import { useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { t } from './i18n';
import { thumbUrl, type Item } from './model';
import Preview from './Preview';

const isProxy = (path: string) => /S03\.MP4$/i.test(path);

/**
 * A whole JPG beats the 1616×1080 preview inside the ARW (or the HEVC preview inside the HIF). The browser turns
 * a whole JPG by its own EXIF (thumb:// is another origin, so `image-orientation: none` is ignored);
 * embedded previews carry no orientation and are turned by the r* class.
 */
function photo(item: Item) {
    const jpg = item.files.find((f) => /\.JPG$/i.test(f.path));
    if (jpg) return { thumb: { path: jpg.path, offset: 0, len: jpg.size, hevc: [] }, className: 'media' };
    return item.thumb && { thumb: item.thumb, className: `media r${item.rotate}` };
}

/** Full-screen view of one tile. Keys are handled by App; remount it (key) for each tile. */
export default function Viewer(props: { item: Item; checked: boolean; onCheck: (v: boolean) => void; onClose: () => void }) {
    const { item } = props;
    // Clip first, proxy next; when both fail (unknown codec or picture-less playback) the system player is offered.
    const videos = item.files.filter((f) => /\.MP4$/i.test(f.path)).sort((a, b) => +isProxy(a.path) - +isProxy(b.path));
    const [tried, setTried] = useState(0);
    const video = videos[tried];
    const failed = () => setTried(tried + 1);
    const player = useRef<HTMLVideoElement>(null);

    const original =
        item.files.find((f) => /\.(ARW|MP4)$/i.test(f.path) && !isProxy(f.path)) ??
        item.files.find((f) => !/\.XML$/i.test(f.path));
    function openExternal() {
        player.current?.pause(); // not two soundtracks at once
        if (original) invoke('open_external', { path: original.path }).catch(alert);
    }
    const image = item.video ? null : photo(item);

    return (
        <div className="viewer">
            {image && <Preview {...image} big />}
            {item.video && video && (
                <video
                    key={video.path}
                    ref={player}
                    className="media"
                    src={convertFileSrc(video.path)}
                    poster={item.thumb ? thumbUrl(item.thumb) : undefined}
                    controls
                    autoPlay
                    onError={failed}
                    // Chromium plays an MP4 whose video codec it lacks as sound only, with a zero width.
                    onLoadedMetadata={(e) => e.currentTarget.videoWidth || failed()}
                />
            )}
            {!image && !video && (
                <div className="media empty">
                    {item.video ? (
                        <button onClick={openExternal}>{t.cantPlay}</button>
                    ) : (
                        <span className="muted">{t.noPreview}</span>
                    )}
                </div>
            )}
            <div className="bar">
                <label>
                    <input type="checkbox" checked={props.checked} onChange={(e) => props.onCheck(e.target.checked)} />
                    {item.name}
                </label>
                <span className="muted">
                    {item.time}
                    {video && isProxy(video.path) && ` · ${t.proxy}`}
                </span>
                <span className="spacer" />
                <button onClick={openExternal}>{t.openExternal}</button>
                <button onClick={props.onClose} title="Esc">
                    ✕
                </button>
            </div>
        </div>
    );
}
