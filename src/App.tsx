import { useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import CopyDialog from './CopyDialog';
import { bytes, total, type FileEntry, type Item, type Library, type Scan, type Thumb } from './model';
import './App.css';

const thumbUrl = (t: Thumb) => convertFileSrc(`${t.offset}-${t.len}-${t.path}`, 'thumb');

const dayTitle = (day: string) =>
    new Date(`${day}T00:00`).toLocaleDateString('ru-RU', {
        weekday: 'short',
        day: 'numeric',
        month: 'long',
        year: 'numeric',
    });

const exts = (item: Item) =>
    [...new Set(item.files.map((f) => f.path.split('.').pop()!.toUpperCase()))].join(' ');

const key = (item: Item) => item.name + item.time;

export default function App() {
    const [scan, setScan] = useState<Scan>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const [root, setRoot] = useState(() => localStorage.getItem('root') ?? '');
    const [lib, setLib] = useState<Library>();
    const [libVersion, setLibVersion] = useState(0); // bumped to look at the library again after copying
    // Tiles whose checkbox differs from the default: checked unless already copied.
    const [toggled, setToggled] = useState(new Set<string>());
    const [dialog, setDialog] = useState(false);

    async function refresh() {
        setBusy(true);
        try {
            setScan(await invoke<Scan>('scan'));
            setToggled(new Set());
            setError(undefined);
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    }

    useEffect(() => {
        refresh();
    }, []);

    useEffect(() => {
        setLib(undefined);
        if (!scan || !root) return;
        let stale = false;
        invoke<Library>('check_library', { root, days: scan.days }).then(
            (l) => {
                if (stale) return;
                setLib(l);
                setError(undefined); // e.g. "folder not found" from a mistyped root before
            },
            (e) => !stale && setError(String(e))
        );
        return () => {
            stale = true;
        };
    }, [scan, root, libVersion]);

    const copied = useMemo(() => new Set(lib?.copied), [lib]);
    const isCopied = (item: Item) => item.files.every((f) => copied.has(f.path));
    const isChecked = (item: Item) => !isCopied(item) !== toggled.has(key(item));

    function check(items: Item[], value: boolean) {
        const next = new Set(toggled);
        for (const item of items) {
            if (value === !isCopied(item)) next.delete(key(item));
            else next.add(key(item));
        }
        setToggled(next);
    }

    // A tile copied in part sends only the rest; one copied in full and checked by hand goes again.
    const toCopy = (item: Item) => {
        const rest = item.files.filter((f) => !copied.has(f.path));
        return rest.length ? rest : item.files;
    };
    const chosen = (scan?.days ?? [])
        .map((d): [string, FileEntry[]] => [d.day, d.items.filter(isChecked).flatMap(toCopy)])
        .filter(([, files]) => files.length);
    const chosenFiles = chosen.flatMap(([, files]) => files);

    function changeRoot(value: string) {
        localStorage.setItem('root', value);
        setRoot(value);
    }

    function closeDialog(didCopy: boolean) {
        setDialog(false);
        if (didCopy) {
            setToggled(new Set());
            setLibVersion((v) => v + 1);
        }
    }

    return (
        <main>
            <header>
                <button onClick={refresh} disabled={busy}>
                    {busy ? 'Поиск…' : 'Обновить'}
                </button>
                <input
                    className="root"
                    placeholder="Папка библиотеки, например e:\FOTO\Sony A7IV\PHOTOS"
                    defaultValue={root}
                    onBlur={(e) => changeRoot(e.target.value.trim())}
                    onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
                />
                <button disabled={!lib || !chosenFiles.length} onClick={() => setDialog(true)}>
                    Копировать {chosenFiles.length ? `${chosenFiles.length} · ${bytes(total(chosenFiles))}` : ''}
                </button>
                <span className="muted">
                    {scan && root && !lib && !error && 'Проверка библиотеки… '}
                    {scan?.cards.length ? `Карты: ${scan.cards.join(', ')}` : ''}
                </span>
            </header>

            {error && <p className="error">{error}</p>}
            {scan && !scan.cards.length && (
                <p className="muted">
                    Карта камеры не найдена. Подключите камеру в режиме MassStorage (MSC) или
                    вставьте карту в картридер и нажмите «Обновить».
                </p>
            )}

            {scan?.days.map((day) => (
                <section key={day.day}>
                    <h2>
                        <label>
                            <input
                                type="checkbox"
                                checked={day.items.every(isChecked)}
                                onChange={(e) => check(day.items, e.target.checked)}
                            />
                            {dayTitle(day.day)}
                        </label>
                        <span className="muted">
                            {day.items.length} · {bytes(total(day.items.flatMap((i) => i.files)))}
                        </span>
                    </h2>
                    <div className="grid">
                        {day.items.map((item) => (
                            <label key={key(item)} className={isCopied(item) ? 'tile copied' : 'tile'}>
                                <div className="thumb">
                                    {item.thumb ? (
                                        <img
                                            loading="lazy"
                                            className={`r${item.rotate}`}
                                            src={thumbUrl(item.thumb)}
                                        />
                                    ) : (
                                        <span className="muted">нет превью</span>
                                    )}
                                    {item.video && <span className="badge">▶</span>}
                                    <input
                                        type="checkbox"
                                        className="check"
                                        checked={isChecked(item)}
                                        onChange={(e) => check([item], e.target.checked)}
                                    />
                                </div>
                                <div className="caption">
                                    {item.name} <span className="muted">{item.time.slice(11, 16)}</span>
                                    <br />
                                    <span className="muted">
                                        {exts(item)}
                                        {isCopied(item) && ' · скопировано'}
                                    </span>
                                </div>
                            </label>
                        ))}
                    </div>
                </section>
            ))}

            {dialog && lib && (
                <CopyDialog root={root} chosen={chosen} existing={lib.existing} onClose={closeDialog} />
            )}
        </main>
    );
}
