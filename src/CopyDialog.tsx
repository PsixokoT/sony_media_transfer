import { useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
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

    const files = targets.flatMap((t) => t.files);
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
        const jobs = targets.map((t) => ({ folder: t.name, files: t.files.map((f) => f.path) }));
        try {
            await invoke('copy_files', { root, jobs, onProgress: channel });
            setResult({ ok: true, text: `Готово: ${files.length} файлов, ${bytes(size)}` });
        } catch (e) {
            setResult({ ok: false, text: String(e) });
        }
    }

    const seconds = (performance.now() - start) / 1000;
    return (
        <div className="overlay">
            <div className="dialog">
                <h2>Копирование в {root}</h2>
                {!progress && (
                    <>
                        <div className="modes">
                            <label>
                                <input type="radio" checked={!perDay} onChange={() => mode(false)} /> Одна папка на
                                диапазон
                            </label>
                            <label>
                                <input type="radio" checked={perDay} onChange={() => mode(true)} /> Папка на каждый
                                день
                            </label>
                        </div>
                        {targets.map((t, i) => (
                            <div className="target" key={t.days[0]}>
                                <input
                                    value={t.name}
                                    autoFocus={i === 0}
                                    onChange={(e) =>
                                        setTargets(targets.map((u, j) => (j === i ? { ...u, name: e.target.value } : u)))
                                    }
                                />
                                <span className="muted">
                                    {existingNames.has(t.name) && 'есть в библиотеке · '}
                                    {t.files.length} файлов · {bytes(total(t.files))}
                                </span>
                            </div>
                        ))}
                    </>
                )}
                {progress && (
                    <>
                        <progress value={progress.bytes} max={size || 1} />
                        <p>
                            {progress.files} из {files.length} файлов · {bytes(progress.bytes)} из {bytes(size)}
                            {seconds > 1 && ` · ${(progress.bytes / 1e6 / seconds).toFixed(0)} МБ/с`}
                        </p>
                    </>
                )}
                {result && <p className={result.ok ? undefined : 'error'}>{result.text}</p>}
                <footer>
                    {!progress && <button onClick={run}>Копировать</button>}
                    <button disabled={progress && !result} onClick={() => props.onClose(!!progress)}>
                        {progress ? 'Закрыть' : 'Отмена'}
                    </button>
                </footer>
            </div>
        </div>
    );
}
