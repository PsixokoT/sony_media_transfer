import { I18n } from 'i18n';

const i18n = new I18n();
i18n.configure({
    locales: ['en'],
    directory: './locales',
    defaultLocale: 'en',
    objectNotation: true,
});

export function t(key: string): string {
    return i18n.__(key);
}
