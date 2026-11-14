import { useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import { t } from './i18n';
import { bytes, total, type FileEntry } from './model';

type Target = { name: string; days: string[]; files: FileEntry[] };
type Progress = { bytes: number; files: number };

const dotted = (day: string) => day.replace(/-/g, '.');

/** Days with an existing library folder go there. The rest get new folders: one per day, or one for their whole range. */
function plan(chosen: [string, FileEntry[]][], existing: Record<string, string>, perDay: boolean): Target[] {
    const fresh = chosen.map(([day]) => day).filter((day) => !existing[day]);
    const range = fresh.length > 1 ? `${dotted(fresh[0])} - ${dotted(fresh[fresh.length - 1])}` : dotted(fresh[0] ?? '');
    const targets = new Map<string, Target>();
    for (const [day, files] of chosen) {
        // Trailing " - " is where the description goes; Rust drops it if left empty.
        const name = existing[day] ?? `${perDay ? dotted(day) : range} - `;
        const target = targets.get(name) ?? { name, days: [], files: [] };
        target.days.push(day);
        target.files.push(...files);
        targets.set(name, target);
    }
    return [...targets.values()];
}

export default function CopyDialog(props: {
    root: string;
    /** Files to copy by day, days in order. */
    chosen: [string, FileEntry[]][];
    existing: Record<string, string>;
    onClose: (copied: boolean) => void;
}) {
    const { root, chosen, existing } = props;
    const [perDay, setPerDay] = useState(false);
    const [targets, setTargets] = useState(() => plan(chosen, existing, false));
    const [progress, setProgress] = useState<Progress>();
    const [start, setStart] = useState(0);
    const [result, setResult] = useState<{ ok: boolean; text: string }>();

    const files = targets.flatMap((target) => target.files);
    const size = total(files);
    const existingNames = new Set(Object.values(existing));

    function mode(value: boolean) {
        setPerDay(value);
        setTargets(plan(chosen, existing, value));
    }

    async function run() {
        const channel = new Channel<Progress>();
        channel.onmessage = setProgress;
        setProgress({ bytes: 0, files: 0 });
        setStart(performance.now());
        const jobs = targets.map((target) => ({ folder: target.name, files: target.files.map((f) => f.path) }));
        try {
            await invoke('copy_files', { root, jobs, onProgress: channel });
            setResult({ ok: true, text: `${t.done}: ${t.files(files.length)}, ${bytes(size)}` });
        } catch (e) {
            setResult({ ok: false, text: String(e) });
        }
    }

    const seconds = (performance.now() - start) / 1000;
    return (
        <div className="overlay">
            <div className="dialog">
                <h2>
                    {t.copyingTo} {root}
                </h2>
                {!progress && (
                    <>
                        <div className="modes">
                            <label>
                                <input type="radio" checked={!perDay} onChange={() => mode(false)} /> {t.oneFolder}
                            </label>
                            <label>
                                <input type="radio" checked={perDay} onChange={() => mode(true)} /> {t.folderPerDay}
                            </label>
                        </div>
                        {targets.map((target, i) => (
                            <div className="target" key={target.days[0]}>
                                <input
                                    value={target.name}
                                    autoFocus={i === 0}
                                    onChange={(e) =>
                                        setTargets(targets.map((u, j) => (j === i ? { ...u, name: e.target.value } : u)))
                                    }
                                />
                                <span className="muted">
                                    {existingNames.has(target.name) && `${t.inLibrary} · `}
                                    {t.files(target.files.length)} · {bytes(total(target.files))}
                                </span>
                            </div>
                        ))}
                    </>
                )}
                {progress && (
                    <>
                        <progress value={progress.bytes} max={size || 1} />
                        <p>
                            {progress.files} {t.of} {t.files(files.length)} · {bytes(progress.bytes)} {t.of}{' '}
                            {bytes(size)}
                            {seconds > 1 && ` · ${(progress.bytes / 1e6 / seconds).toFixed(0)} ${t.mbPerSecond}`}
                        </p>
                    </>
                )}
                {result && <p className={result.ok ? undefined : 'error'}>{result.text}</p>}
                <footer>
                    {!progress && <button onClick={run}>{t.copy}</button>}
                    <button disabled={progress && !result} onClick={() => props.onClose(!!progress)}>
                        {progress ? t.close : t.cancel}
                    </button>
                </footer>
            </div>
        </div>
    );
}
