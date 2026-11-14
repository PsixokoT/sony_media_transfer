import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import CopyDialog from './CopyDialog';
import Preview from './Preview';
import Settings from './Settings';
import Viewer from './Viewer';
import { setLang, t, type Lang } from './i18n';
import { Icon } from './icons';
import { bytes, isProxy, openFolder, total, type FileEntry, type Item, type Library, type Scan } from './model';
import './App.css';

const dayTitle = (day: string) =>
    new Date(`${day}T00:00`).toLocaleDateString(t.locale, {
        weekday: 'short',
        day: 'numeric',
        month: 'long',
        year: 'numeric',
    });

/** "ARW JPG", "MP4 XML прокси". */
function exts(item: Item) {
    const list = new Set(item.files.map((f) => f.path.split('.').pop()!.toUpperCase()));
    if (item.files.some((f) => isProxy(f.path))) list.add(t.proxy);
    return [...list].join(' ');
}

const isRaw = (f: FileEntry) => /\.ARW$/i.test(f.path);

const key = (item: Item) => item.name + item.time;

/** Local date `daysAgo` days back, YYYY-MM-DD: compares as a string. */
const date = (daysAgo = 0) => new Date(Date.now() - daysAgo * 864e5).toLocaleDateString('sv-SE');

/** Hidden tiles: key -> the day last seen on a card. Not seen for 30 days (the card was formatted): forgotten. */
function loadHidden(): Record<string, string> {
    const saved: Record<string, string> = JSON.parse(localStorage.getItem('hidden') ?? '{}');
    return Object.fromEntries(Object.entries(saved).filter(([, seen]) => seen >= date(30)));
}

export default function App() {
    const [scan, setScan] = useState<Scan>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const [root, setRoot] = useState(() => localStorage.getItem('root') ?? '');
    const [withRaw, setWithRaw] = useState(() => localStorage.getItem('jpgWithRaw') === 'true');
    const [lib, setLib] = useState<Library>();
    const [libVersion, setLibVersion] = useState(0); // bumped to look at the library again after copying
    // Tiles whose checkbox differs from the default: checked unless already copied.
    const [toggled, setToggled] = useState(new Set<string>());
    const [hidden, setHidden] = useState(loadHidden);
    const [showHidden, setShowHidden] = useState(false);
    const [dialog, setDialog] = useState(false);
    // Tile keys: the cursor, where a Shift range starts, and the selection Space and checkboxes act on.
    const [current, setCurrent] = useState<string>();
    const [anchor, setAnchor] = useState<string>();
    const [selected, setSelected] = useState(new Set<string>());
    const [viewing, setViewing] = useState(false);
    const [settings, setSettings] = useState(false);
    const [ejected, setEjected] = useState(false);
    const [, setLangState] = useState<Lang>(); // only to re-render after setLang

    async function refresh() {
        setBusy(true);
        try {
            const found = await invoke<Scan>('scan');
            setScan(found);
            setHidden((h) => {
                const next = { ...h };
                for (const day of found.days) for (const item of day.items) if (key(item) in next) next[key(item)] = date();
                return next;
            });
            if (found.cards.length) setEjected(false);
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

    useEffect(() => localStorage.setItem('hidden', JSON.stringify(hidden)), [hidden]);

    // Hot plug: the drive list is polled, and the cards are read again only when it changes.
    useEffect(() => {
        if (busy) return;
        let stale = false;
        const timer = setInterval(async () => {
            const cards = await invoke<string[]>('cards');
            if (!stale && cards.join() !== scan?.cards.join()) refresh();
        }, 2000);
        return () => {
            stale = true;
            clearInterval(timer);
        };
    }, [scan, busy]);

    async function eject() {
        try {
            await invoke('eject');
            setEjected(true);
            refresh();
        } catch (e) {
            setError(String(e)); // not refreshed: that would clear the error
        }
    }

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
    // What a tile needs in the library. Unless Settings say otherwise, a frame with an ARW needs only the ARW.
    const needed = (item: Item) => (withRaw || !item.files.some(isRaw) ? item.files : item.files.filter(isRaw));
    const isCopied = (item: Item) => needed(item).every((f) => copied.has(f.path));
    const isChecked = (item: Item) => !isCopied(item) !== toggled.has(key(item));
    // Hidden tiles are rejects: never copied, counted, checked or viewed.
    const isHidden = (item: Item) => key(item) in hidden;
    const live = (items: Item[]) => items.filter((i) => !isHidden(i));

    function check(items: Item[], value: boolean) {
        const next = new Set(toggled);
        for (const item of live(items)) {
            if (value === !isCopied(item)) next.delete(key(item));
            else next.add(key(item));
        }
        setToggled(next);
    }

    // A tile copied in part sends only the rest; one copied in full and checked by hand goes again.
    const toCopy = (item: Item) => {
        const files = needed(item);
        const rest = files.filter((f) => !copied.has(f.path));
        return rest.length ? rest : files;
    };
    const chosen = (scan?.days ?? [])
        .map((d): [string, FileEntry[]] => [d.day, live(d.items).filter(isChecked).flatMap(toCopy)])
        .filter(([, files]) => files.length);
    const chosenFiles = chosen.flatMap(([, files]) => files);

    const flat = useMemo(() => scan?.days.flatMap((d) => d.items) ?? [], [scan]);
    const hiddenHere = flat.filter(isHidden).length;
    if (showHidden && !hiddenHere) setShowHidden(false); // the checkbox is gone; the next hidden tile isn't shown
    // What the cursor walks and Ctrl+A selects: the grid, or in the viewer only the tiles not hidden.
    const list = useMemo(
        () => (showHidden && !viewing ? flat : flat.filter((i) => !(key(i) in hidden))),
        [flat, hidden, showHidden, viewing]
    );
    const index = useMemo(() => new Map(list.map((item, i) => [key(item), i])), [list]);
    const pos = index.get(current ?? '') ?? -1;

    /** Cursor to tile i, the way Explorer and Lightroom do it: alone it selects only that tile,
     *  Shift selects from the anchor to it, Ctrl adds it to the selection or takes it out. */
    function select(i: number, mods: { shiftKey: boolean; ctrlKey: boolean }) {
        const k = key(list[i]);
        setCurrent(k);
        if (mods.shiftKey) {
            const a = index.get(anchor ?? '') ?? i;
            setSelected(new Set(list.slice(Math.min(a, i), Math.max(a, i) + 1).map(key)));
            return;
        }
        setAnchor(k);
        const next = new Set(mods.ctrlKey ? selected : []);
        if (!next.delete(k)) next.add(k);
        setSelected(next);
    }

    // The selection, or the cursor tile when nothing is selected.
    const targets = () => (selected.size ? list.filter((i) => selected.has(key(i))) : list.slice(pos, pos + 1));

    /** Hides the tiles, or unhides them if all are hidden already, like Space does with checkboxes. */
    function hide(items: Item[]) {
        const back = items.every(isHidden);
        const next = { ...hidden };
        for (const item of items) {
            if (back) delete next[key(item)];
            else next[key(item)] = date();
        }
        setHidden(next);
        setSelected(new Set());
        if (back || !current || !(current in next)) return;
        // The cursor goes on to the next (older) tile still shown, else the one before; with neither the viewer closes.
        const i = flat.findIndex((item) => key(item) === current);
        const shown = (item: Item) => !(key(item) in next);
        const to = flat.slice(i + 1).find(shown) ?? flat.slice(0, i).reverse().find(shown);
        setCurrent(to && key(to));
        setAnchor(to && key(to));
        if (!to) setViewing(false);
    }

    // ← → move the cursor (in the grid and in the viewer), Shift+← → extend the selection, Ctrl+A selects all,
    // Space checks or unchecks the selection, Delete hides or unhides it, Enter opens, Esc closes.
    // No deps: it subscribes again on every render, so it always sees the current state.
    useEffect(() => {
        function onKey(e: KeyboardEvent) {
            const target = e.target as HTMLElement;
            if (dialog || settings || target.matches('input:not([type=checkbox])')) return; // typing, or a dialog
            if (target.matches('button') && (e.key === 'Enter' || e.key === ' ')) return; // pressing a button
            const move = (by: number) =>
                list.length && select(Math.min(Math.max(pos + by, 0), list.length - 1), { shiftKey: e.shiftKey, ctrlKey: false });
            const t = targets();
            if (e.key === 'ArrowLeft') move(-1);
            else if (e.key === 'ArrowRight') move(1);
            else if (e.code === 'KeyA' && e.ctrlKey) setSelected(new Set(index.keys())); // code: also on a Russian layout
            else if (e.key === ' ' && t.length) check(t, !live(t).every(isChecked)); // mixed: check them all
            else if (e.key === 'Delete' && t.length) hide(t);
            else if (e.key === 'Enter' && list[pos] && !isHidden(list[pos])) setViewing(true);
            else if (e.key === 'Escape') setViewing(false);
            else return;
            e.preventDefault(); // no page scroll on Space, no native checkbox toggle or video seek on top of ours
        }
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    });

    useEffect(() => document.querySelector('.tile.current')?.scrollIntoView({ block: 'nearest' }), [current]);

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

    const targeted = targets();
    const unhiding = targeted.length > 0 && targeted.every(isHidden);
    return (
        <main>
            <header>
                <button onClick={refresh} disabled={busy}>
                    <Icon name="refresh" />
                    {busy ? t.searching : t.refresh}
                </button>
                <button onClick={eject} disabled={busy || !scan?.cards.length}>
                    <Icon name="eject" />
                    {t.eject}
                </button>
                <button disabled={!lib || !chosenFiles.length} onClick={() => setDialog(true)}>
                    <Icon name="copy" />
                    {t.copy} {chosenFiles.length ? `${chosenFiles.length} · ${bytes(total(chosenFiles))}` : ''}
                </button>
                <button disabled={!targeted.length} onClick={() => hide(targeted)} title="Delete">
                    <Icon name={unhiding ? 'unhide' : 'hide'} />
                    {unhiding ? t.unhide : t.hide}
                </button>
                {hiddenHere > 0 && (
                    <label>
                        <input type="checkbox" checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} />
                        {t.showHidden(hiddenHere)}
                    </label>
                )}
                <span className="muted">
                    {scan && root && !lib && !error && `${t.checkingLibrary} `}
                    {scan?.cards.length ? `${t.cards}: ${scan.cards.join(', ')}` : ''}
                </span>
                <span className="spacer" />
                {root ? (
                    <>
                        <span className="muted root" title={root}>
                            {root}
                        </span>
                        <button className="icon-button" title={t.openFolder} onClick={() => openFolder(root)}>
                            <Icon name="openFolder" />
                        </button>
                    </>
                ) : (
                    <button onClick={() => setSettings(true)}>
                        <Icon name="choose" />
                        {t.chooseFolder}
                    </button>
                )}
                <button onClick={() => setSettings(true)}>
                    <Icon name="settings" />
                    {t.settings}
                </button>
            </header>

            {error && <p className="error">{error}</p>}
            {scan && !scan.cards.length && <p className="muted">{ejected ? t.ejected : t.noCard}</p>}

            {scan?.days.map((day) => {
                const items = showHidden ? day.items : live(day.items);
                const counted = live(day.items);
                return items.length > 0 && (
                    <section key={day.day}>
                        <h2>
                            <label>
                                <input
                                    type="checkbox"
                                    checked={counted.length > 0 && counted.every(isChecked)}
                                    onChange={(e) => check(counted, e.target.checked)}
                                />
                                {dayTitle(day.day)}
                            </label>
                            <span className="muted">
                                {counted.length} · {bytes(total(counted.flatMap((i) => i.files)))}
                            </span>
                        </h2>
                        <div className="grid">
                            {items.map((item) => (
                                <div
                                    key={key(item)}
                                    className={[
                                        'tile',
                                        isCopied(item) && 'copied',
                                        isHidden(item) && 'hidden',
                                        key(item) === current && 'current',
                                        selected.has(key(item)) && 'selected',
                                    ]
                                        .filter(Boolean)
                                        .join(' ')}
                                    onClick={(e) => select(index.get(key(item))!, e)}
                                    onDoubleClick={() => !isHidden(item) && setViewing(true)}
                                >
                                    <div className="thumb">
                                        {item.thumb ? (
                                            <Preview thumb={item.thumb} className={`r${item.rotate}`} />
                                        ) : (
                                            <span className="muted">{t.noPreview}</span>
                                        )}
                                        {item.video && <span className="badge">▶</span>}
                                        {isHidden(item) ? (
                                            <button
                                                className="icon-button unhide"
                                                title={t.unhide}
                                                onClick={(e) => {
                                                    e.stopPropagation(); // unhides, doesn't select
                                                    hide([item]);
                                                }}
                                            >
                                                <Icon name="unhide" />
                                            </button>
                                        ) : (
                                            <input
                                                type="checkbox"
                                                className="check"
                                                checked={isChecked(item)}
                                                // On a selected tile it checks the whole selection, like in Lightroom.
                                                onChange={(e) => check(selected.has(key(item)) ? targets() : [item], e.target.checked)}
                                                onClick={(e) => e.stopPropagation()} // checks, doesn't select
                                            />
                                        )}
                                    </div>
                                    <div className="caption">
                                        {item.name} <span className="muted">{item.time.slice(11, 16)}</span>
                                        <br />
                                        <span className="muted">
                                            {exts(item)}
                                            {isCopied(item) && ` · ${t.copied}`}
                                            {isHidden(item) && ` · ${t.hiddenMark}`}
                                        </span>
                                    </div>
                                </div>
                            ))}
                        </div>
                    </section>
                );
            })}

            {viewing && list[pos] && (
                <Viewer
                    key={current}
                    item={list[pos]}
                    checked={isChecked(list[pos])}
                    onCheck={(v) => check([list[pos]], v)}
                    onClose={() => setViewing(false)}
                />
            )}

            {dialog && scan && lib && (
                <CopyDialog root={root} days={scan.days} chosen={chosen} existing={lib.existing} onClose={closeDialog} />
            )}

            {settings && (
                <Settings
                    root={root}
                    onRoot={changeRoot}
                    withRaw={withRaw}
                    onWithRaw={(v) => {
                        localStorage.setItem('jpgWithRaw', String(v));
                        setWithRaw(v);
                        setToggled(new Set()); // hand-set checkboxes would flip where "copied" changes
                    }}
                    hiddenCount={Object.keys(hidden).length}
                    onUnhideAll={() => setHidden({})}
                    onLang={(l) => {
                        setLang(l);
                        setLangState(l);
                    }}
                    onClose={() => setSettings(false)}
                />
            )}
        </main>
    );
}
