import { useEffect, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import './App.css';

type FileEntry = { path: string; size: number };
type Thumb = { path: string; offset: number; len: number };
type Item = {
    name: string;
    video: boolean;
    time: string;
    files: FileEntry[];
    thumb: Thumb | null;
    rotate: number;
};
type Day = { day: string; items: Item[] };
type Scan = { cards: string[]; days: Day[] };

const thumbUrl = (t: Thumb) => convertFileSrc(`${t.offset}-${t.len}-${t.path}`, 'thumb');

const size = (items: Item[]) => {
    const bytes = items.flatMap((i) => i.files).reduce((sum, f) => sum + f.size, 0);
    return bytes > 1e9 ? `${(bytes / 1e9).toFixed(1)} ГБ` : `${(bytes / 1e6).toFixed(0)} МБ`;
};

const dayTitle = (day: string) =>
    new Date(`${day}T00:00`).toLocaleDateString('ru-RU', {
        weekday: 'short',
        day: 'numeric',
        month: 'long',
        year: 'numeric',
    });

const exts = (item: Item) =>
    [...new Set(item.files.map((f) => f.path.split('.').pop()!.toUpperCase()))].join(' ');

export default function App() {
    const [scan, setScan] = useState<Scan>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);

    async function refresh() {
        setBusy(true);
        try {
            setScan(await invoke<Scan>('scan'));
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

    return (
        <main>
            <header>
                <button onClick={refresh} disabled={busy}>
                    {busy ? 'Поиск…' : 'Обновить'}
                </button>
                {scan && (
                    <span className="muted">
                        {scan.cards.length ? `Карты: ${scan.cards.join(', ')}` : ''}
                    </span>
                )}
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
                        {dayTitle(day.day)}
                        <span className="muted">
                            {day.items.length} · {size(day.items)}
                        </span>
                    </h2>
                    <div className="grid">
                        {day.items.map((item) => (
                            <figure key={item.name + item.time}>
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
                                </div>
                                <figcaption>
                                    {item.name} <span className="muted">{item.time.slice(11, 16)}</span>
                                    <br />
                                    <span className="muted">{exts(item)}</span>
                                </figcaption>
                            </figure>
                        ))}
                    </div>
                </section>
            ))}
        </main>
    );
}
