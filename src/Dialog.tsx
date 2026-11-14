import { useEffect, type ReactNode } from 'react';
import { t } from './i18n';
import { Icon } from './icons';

/**
 * A popup: title, ✕, body, footer. Esc works like ✕; a click outside does nothing.
 * Without onClose (while copying) it can't be closed.
 */
export default function Dialog(props: { title: ReactNode; onClose?: () => void; footer?: ReactNode; children: ReactNode }) {
    const { onClose } = props;
    useEffect(() => {
        if (!onClose) return;
        function onKey(e: KeyboardEvent) {
            if (e.key !== 'Escape') return;
            (document.activeElement as HTMLElement | null)?.blur(); // a field being typed in is saved, as with a click on ✕
            onClose!();
        }
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [onClose]);

    return (
        <div className="overlay">
            <div className="dialog">
                <header>
                    <h2>{props.title}</h2>
                    <button className="icon-button" title={`${t.close} (Esc)`} disabled={!onClose} onClick={onClose}>
                        <Icon name="close" />
                    </button>
                </header>
                {props.children}
                {props.footer && <footer>{props.footer}</footer>}
            </div>
        </div>
    );
}
