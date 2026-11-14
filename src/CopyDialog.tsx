import { useRef, useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import Dialog from './Dialog';
import { t } from './i18n';
import { Icon } from './icons';
import { bytes, openFolder, total, type Day, type FileEntry, type Library } from './model';
import { FolderField } from './Settings';

type Target = { name: string; days: string[]; files: FileEntry[]; edited?: boolean };
type Progress = { bytes: number; files: number };

const dotted = (day: string) => day.replace(/-/g, '.');
/** What Rust's folder_name does: an empty description leaves no trailing " - ". */
const trimName = (name: string) => name.trim().replace(/[ .-]+$/, '');

/** Days with an existing library folder go there. The rest get new folders: one per day, or one for their whole range. */
function plan(chosen: [string, FileEntry[]][], existing: Record<string, string>, perDay: boolean): Target[] {
    const fresh = chosen.map(([day]) => day).filter((day) => !existing[day]).sort(); // the range reads earliest - latest
    const range = fresh.length > 1 ? `${dotted(fresh[0])} - ${dotted(fresh[fresh.length - 1])}` : dotted(fresh[0] ?? '');
    const targets = new Map<string, Target>();
    for (const [day, files] of chosen) {
        const name = existing[day] ?? (perDay ? dotted(day) : range);
        const target = targets.get(name) ?? { name, days: [], files: [] };
        target.days.push(day);
        target.files.push(...files);
        targets.set(name, target);
    }
    return [...targets.values()];
}

export default function CopyDialog(props: {
    root: string;
    /** The card's days as `scan` gave them, to check another library root. */
    days: Day[];
    /** Files to copy by day, days in order. */
    chosen: [string, FileEntry[]][];
    existing: Record<string, string>;
    onClose: (copied: boolean) => void;
}) {
    const { chosen } = props;
    // The root for this copy only; Settings keep theirs.
    const [root, setRoot] = useState(props.root);
    const [existing, setExisting] = useState(props.existing);
    const [checking, setChecking] = useState(false);
    const [rootError, setRootError] = useState<string>();
    const asked = useRef(root); // the latest root asked about: an older answer is dropped
    const [perDay, setPerDay] = useState(false);
    const [targets, setTargets] = useState(() => plan(chosen, props.existing, false));
    const [progress, setProgress] = useState<Progress>();
    const [start, setStart] = useState(0);
    const [result, setResult] = useState<{ ok: boolean; text: string; folders: string[] }>();

    const files = targets.flatMap((target) => target.files);
    const size = total(files);
    const existingNames = new Set(Object.values(existing));

    function mode(value: boolean) {
        setPerDay(value);
        setTargets(plan(chosen, existing, value));
    }

    async function changeRoot(value: string) {
        if (value === asked.current) return; // leaving the field without a change
        asked.current = value;
        setRoot(value);
        setChecking(true);
        setRootError(undefined);
        try {
            const lib = await invoke<Library>('check_library', { root: value, days: props.days });
            if (asked.current !== value) return;
            setExisting(lib.existing);
            setTargets(plan(chosen, lib.existing, perDay)); // perDay is as it was: the fieldset is disabled meanwhile
        } catch (e) {
            if (asked.current === value) setRootError(String(e));
        } finally {
            if (asked.current === value) setChecking(false);
        }
    }

    async function run() {
        const channel = new Channel<Progress>();
        channel.onmessage = setProgress;
        setProgress({ bytes: 0, files: 0 });
        setStart(performance.now());
        const jobs = targets.map((target) => ({ folder: target.name, files: target.files.map((f) => f.path) }));
        try {
            const folders = await invoke<string[]>('copy_files', { root, jobs, onProgress: channel });
            setResult({ ok: true, text: `${t.done}: ${t.files(files.length)}, ${bytes(size)}`, folders });
        } catch (e) {
            setResult({ ok: false, text: String(e), folders: [] });
        }
    }

    const seconds = (performance.now() - start) / 1000;
    return (
        <Dialog
            title={t.copying}
            // No way to stop a copy halfway, so no closing either.
            onClose={progress && !result ? undefined : () => props.onClose(!!progress)}
            footer={
                !progress && (
                    <button onClick={run} disabled={checking || !!rootError}>
                        <Icon name="copy" />
                        {t.copy}
                    </button>
                )
            }
        >
            {!progress && (
                <>
                    <p>{t.library}</p>
                    <FolderField value={root} onChange={changeRoot} />
                    {rootError && <p className="error">{rootError}</p>}
                    {checking && <p className="muted">{t.checkingLibrary}</p>}
                    <fieldset disabled={checking}>
                        <div className="modes">
                            <label>
                                <input type="radio" checked={!perDay} onChange={() => mode(false)} /> {t.oneFolder}
                            </label>
                            <label>
                                <input type="radio" checked={perDay} onChange={() => mode(true)} /> {t.folderPerDay}
                            </label>
                        </div>
                        {targets.map((target, i) => {
                            const update = (change: Partial<Target>) =>
                                setTargets((all) => all.map((u, j) => (j === i ? { ...u, ...change } : u)));
                            return (
                                <div className="target" key={target.days[0]}>
                                    <input
                                        value={target.name}
                                        autoFocus={i === 0}
                                        // " - " is where the description goes: offered on focus, dropped if left empty.
                                        onFocus={() =>
                                            !target.edited &&
                                            !existingNames.has(target.name) &&
                                            update({ name: `${target.name} - ` })
                                        }
                                        onChange={(e) => update({ name: e.target.value, edited: true })}
                                        onBlur={() => update({ name: trimName(target.name) })}
                                    />
                                    <span className="muted">
                                        {existingNames.has(target.name) && `${t.inLibrary} · `}
                                        {t.files(target.files.length)} · {bytes(total(target.files))}
                                    </span>
                                </div>
                            );
                        })}
                    </fieldset>
                </>
            )}
            {progress && (
                <>
                    <progress value={progress.bytes} max={size || 1} />
                    <p>
                        {progress.files} {t.of} {t.files(files.length)} · {bytes(progress.bytes)} {t.of} {bytes(size)}
                        {seconds > 1 && ` · ${(progress.bytes / 1e6 / seconds).toFixed(0)} ${t.mbPerSecond}`}
                    </p>
                </>
            )}
            {result && <p className={result.ok ? undefined : 'error'}>{result.text}</p>}
            {result?.folders.map((folder) => (
                <div className="folder" key={folder}>
                    {folder}
                    <button className="icon-button" title={t.openFolder} onClick={() => openFolder(folder)}>
                        <Icon name="openFolder" />
                    </button>
                </div>
            ))}
        </Dialog>
    );
}
