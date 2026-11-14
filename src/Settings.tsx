import { open } from '@tauri-apps/plugin-dialog';
import { lang, t, type Lang } from './i18n';

/** Changes apply at once and are saved; there's nothing to confirm. */
export default function Settings(props: {
    root: string;
    onRoot: (root: string) => void;
    onLang: (lang: Lang) => void;
    onClose: () => void;
}) {
    async function pick() {
        const dir = await open({ directory: true, defaultPath: props.root || undefined });
        if (dir) props.onRoot(dir);
    }

    return (
        <div className="overlay">
            <div className="dialog">
                <h2>{t.settings}</h2>
                <p>{t.library}</p>
                <div className="target">
                    <input
                        key={props.root} // shows a folder picked with the button
                        defaultValue={props.root}
                        placeholder={t.libraryExample}
                        onBlur={(e) => props.onRoot(e.target.value.trim())}
                        onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
                    />
                    <button onClick={pick}>{t.choose}</button>
                </div>
                <p>
                    <label>
                        {t.language}{' '}
                        <select value={lang} onChange={(e) => props.onLang(e.target.value as Lang)}>
                            <option value="ru">Русский</option>
                            <option value="en">English</option>
                        </select>
                    </label>
                </p>
                <footer>
                    <button onClick={props.onClose}>{t.close}</button>
                </footer>
            </div>
        </div>
    );
}
