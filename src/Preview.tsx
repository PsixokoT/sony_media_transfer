import { useEffect, useState } from 'react';
import { thumbUrl, type Hevc, type Thumb } from './model';

/**
 * A HIF's previews are HEVC (Sony: 1616×1080 in the shot's chroma, 320×212 always 4:2:0). WebView2 decodes HEVC
 * with WebCodecs, in hardware and only 4:2:0, so a HEIF 4:2:2 shot gets the 320×212 one in the viewer too.
 * Each decoded image becomes a JPEG blob URL.
 * shortcut: blob URLs are kept for the whole session and the grid decodes every HIF tile, not just the visible ones;
 * fine at a few ms and ~20 KB a tile, add an IntersectionObserver if a card holds thousands of HIF.
 */
const decoded = new Map<string, Promise<string>>();
// One decode at a time, so a grid of HIF doesn't open hundreds of hardware decoders. The viewer has its own
// queue so it doesn't wait for the grid.
const queues = { small: Promise.resolve(), big: Promise.resolve() };

function hevcUrl(thumb: Thumb, size: 'small' | 'big'): Promise<string> {
    const key = `${size}-${thumbUrl(thumb)}`;
    let url = decoded.get(key);
    if (!url) {
        const tries = size === 'big' ? thumb.hevc : [...thumb.hevc].reverse(); // largest first in Rust
        url = queues[size].then(async () => {
            for (const hevc of tries) {
                try {
                    return await decode(thumb.path, hevc);
                } catch {
                    // unsupported chroma or no HEVC decoder: try the next one
                }
            }
            return thumbUrl(thumb); // the JPEG
        });
        queues[size] = url.then(() => {});
        decoded.set(key, url);
    }
    return url;
}

async function decode(path: string, hevc: Hevc): Promise<string> {
    const c = hevc.config;
    // Sony writes the hvcC compatibility flags in the wrong byte order, so the codec string is rebuilt from
    // profile, tier and level: hvc1.<profile>.<its compatibility flag>.<L|H><level>.B0
    const codec = `hvc1.${c[1] & 31}.${(1 << (c[1] & 31)).toString(16)}.${c[1] & 32 ? 'H' : 'L'}${c[12]}.B0`;
    // Without the size Chromium takes 1280×720 as the picture's visible part; with it, it crops the padded
    // coded picture (1616×1080 is coded as 1664×1088) to the real one.
    const config = { codec, description: new Uint8Array(c), codedWidth: hevc.width, codedHeight: hevc.height };
    if (!(await VideoDecoder.isConfigSupported(config)).supported) throw new Error(`${codec} not supported`);
    const data = await (await fetch(thumbUrl({ path, offset: hevc.offset, len: hevc.len }))).arrayBuffer();
    const canvas = new OffscreenCanvas(hevc.width, hevc.height);
    let drawn = false;
    const decoder = new VideoDecoder({
        output: (frame) => {
            canvas.getContext('2d')!.drawImage(frame, 0, 0);
            frame.close();
            drawn = true;
        },
        error: () => {}, // flush() rejects as well
    });
    try {
        decoder.configure(config);
        decoder.decode(new EncodedVideoChunk({ type: 'key', timestamp: 0, data }));
        await decoder.flush();
    } finally {
        if (decoder.state !== 'closed') decoder.close();
    }
    if (!drawn) throw new Error('no frame');
    return URL.createObjectURL(await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.9 }));
}

/** A tile's preview. `big` is for the viewer: the largest HEVC image of a HIF that decodes. */
export default function Preview(props: { thumb: Thumb; className: string; big?: boolean }) {
    const { thumb, big } = props;
    const jpeg = thumbUrl(thumb);
    const [hevc, setHevc] = useState<string>();
    useEffect(() => {
        let live = true;
        if (thumb.hevc.length) hevcUrl(thumb, big ? 'big' : 'small').then((url) => live && setHevc(url));
        return () => void (live = false);
    }, [jpeg, big]);
    const src = thumb.hevc.length ? hevc : jpeg;
    return src ? <img loading="lazy" className={props.className} src={src} /> : null;
}
