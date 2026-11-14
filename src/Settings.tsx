import { open } from '@tauri-apps/plugin-dialog';
import Dialog from './Dialog';
import { lang, t, type Lang } from './i18n';
import { Icon } from './icons';

/** A folder path, typed (taken on Enter or leaving the field) or picked with the standard dialog. */
export function FolderField(props: { value: string; onChange: (path: string) => void }) {
    async function pick() {
        const dir = await open({ directory: true, defaultPath: props.value || undefined });
        if (dir) props.onChange(dir);
    }

    return (
        <div className="target">
            <input
                key={props.value} // shows a folder picked with the button
                defaultValue={props.value}
                placeholder={t.libraryExample}
                onBlur={(e) => props.onChange(e.target.value.trim())}
                onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
            />
            <button onClick={pick}>
                <Icon name="choose" />
                {t.choose}
            </button>
        </div>
    );
}

/** Changes apply at once and are saved; there's nothing to confirm. */
export default function Settings(props: {
    root: string;
    onRoot: (root: string) => void;
    withRaw: boolean;
    onWithRaw: (value: boolean) => void;
    onLang: (lang: Lang) => void;
    onClose: () => void;
}) {
    return (
        <Dialog title={t.settings} onClose={props.onClose}>
            <p>{t.library}</p>
            <FolderField value={props.root} onChange={props.onRoot} />
            <p>
                <label>
                    <input type="checkbox" checked={props.withRaw} onChange={(e) => props.onWithRaw(e.target.checked)} />{' '}
                    {t.jpgWithRaw}
                </label>
            </p>
            <p>
                <label>
                    {t.language}{' '}
                    <select value={lang} onChange={(e) => props.onLang(e.target.value as Lang)}>
                        <option value="ru">Русский</option>
                        <option value="en">English</option>
                    </select>
                </label>
            </p>
        </Dialog>
    );
}
