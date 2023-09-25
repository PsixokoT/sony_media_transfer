import { I18n } from 'i18n';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const i18n = new I18n();
i18n.configure({
    locales: ['en'],
    directory: join(__dirname, '../locales'),
    defaultLocale: 'en',
    objectNotation: true,
});

export function t(key: string, ...args:string[]): string {
    return i18n.__(key, ...args);
}
