import { invoke } from '@tauri-apps/api/core';

const ru = {
    locale: 'ru-RU',
    gb: 'ГБ',
    mb: 'МБ',
    mbPerSecond: 'МБ/с',
    files: (n: number) => `${n} файлов`,
    of: 'из',
    // Main window
    refresh: 'Обновить',
    searching: 'Поиск…',
    eject: 'Извлечь',
    copy: 'Копировать',
    settings: 'Настройки',
    noRoot: 'Выберите папку библиотеки в настройках.',
    checkingLibrary: 'Проверка библиотеки…',
    cards: 'Карты',
    noCard: 'Карта камеры не найдена. Подключите камеру в режиме MassStorage (MSC) или вставьте карту в картридер: она появится здесь сама.',
    ejected: 'Камера извлечена, её можно отключать.',
    noPreview: 'нет превью',
    copied: 'скопировано',
    // Copy dialog
    copyingTo: 'Копирование в',
    oneFolder: 'Одна папка на диапазон',
    folderPerDay: 'Папка на каждый день',
    inLibrary: 'есть в библиотеке',
    done: 'Готово',
    close: 'Закрыть',
    cancel: 'Отмена',
    // Viewer
    cantPlay: 'Клип здесь не воспроизводится. Открыть в системном плеере',
    proxy: 'прокси',
    openExternal: 'Открыть во внешней программе',
    // Settings
    library: 'Папка библиотеки',
    libraryExample: 'например e:\\FOTO\\Sony A7IV\\PHOTOS',
    choose: 'Выбрать…',
    language: 'Язык',
};

const en: typeof ru = {
    locale: 'en-GB',
    gb: 'GB',
    mb: 'MB',
    mbPerSecond: 'MB/s',
    files: (n) => `${n} file${n === 1 ? '' : 's'}`,
    of: 'of',
    refresh: 'Refresh',
    searching: 'Searching…',
    eject: 'Eject',
    copy: 'Copy',
    settings: 'Settings',
    noRoot: 'Choose the library folder in Settings.',
    checkingLibrary: 'Checking the library…',
    cards: 'Cards',
    noCard: 'No camera card found. Connect the camera in MassStorage (MSC) mode or put the card into a card reader: it will show up here by itself.',
    ejected: 'The camera has been ejected and can be unplugged.',
    noPreview: 'no preview',
    copied: 'copied',
    copyingTo: 'Copying to',
    oneFolder: 'One folder for the range',
    folderPerDay: 'A folder per day',
    inLibrary: 'in the library',
    done: 'Done',
    close: 'Close',
    cancel: 'Cancel',
    cantPlay: "The clip doesn't play here. Open in the system player",
    proxy: 'proxy',
    openExternal: 'Open in an external program',
    library: 'Library folder',
    libraryExample: 'e.g. e:\\FOTO\\Sony A7IV\\PHOTOS',
    choose: 'Choose…',
    language: 'Language',
};

const dict = { ru, en };
export type Lang = keyof typeof dict;

// The choice made in Settings, or else the Windows language (WebView2 takes it from the OS).
const saved = localStorage.getItem('lang');
export let lang: Lang = saved === 'ru' || saved === 'en' ? saved : navigator.language.startsWith('ru') ? 'ru' : 'en';
/** Read at render time: `export let` is a live binding, so a re-render after setLang shows the new language. */
export let t = dict[lang];
invoke('set_language', { lang }); // Rust's own error messages

export function setLang(value: Lang) {
    lang = value;
    t = dict[value];
    localStorage.setItem('lang', value);
    invoke('set_language', { lang: value });
}
