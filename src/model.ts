import { convertFileSrc } from '@tauri-apps/api/core';
import { t } from './i18n';

// Shapes of what the Rust commands return.

export type FileEntry = { path: string; size: number };
export type Thumb = { path: string; offset: number; len: number };
export type Item = {
    name: string;
    video: boolean;
    time: string;
    files: FileEntry[];
    thumb: Thumb | null;
    rotate: number;
};
export type Day = { day: string; items: Item[] };
export type Scan = { cards: string[]; days: Day[] };
/** Card files already in the library; day -> the existing library folder for it. */
export type Library = { copied: string[]; existing: Record<string, string> };

export const thumbUrl = (t: Thumb) => convertFileSrc(`${t.offset}-${t.len}-${t.path}`, 'thumb');

export const total =(files: FileEntry[]) => files.reduce((sum, f) => sum + f.size, 0);

export const bytes = (n: number) => (n > 1e9 ? `${(n / 1e9).toFixed(1)} ${t.gb}` : `${(n / 1e6).toFixed(0)} ${t.mb}`);
