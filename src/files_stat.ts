import { AsciiTable3 } from 'ascii-table3';
import { Stats } from 'fs';
import { lstat } from 'fs/promises';
import strftime from 'strftime';
import { store } from './storage.js';
import { t } from './translate.js';

export type FileStat = { path: string; stat: Stats; day: string };
export type StatObject = {
    stats: FileStat[];
    byDay: Record<string, FileStat[]>;
    totalSize: number;
    log: () => string;
};

export async function getStat(files: string[]): Promise<StatObject> {
    const stats = await Promise.all(
        files.map((path) =>
            lstat(path).then((stat) => {
                const day = strftime(store.format, stat.birthtime);
                return { path, stat, day };
            })
        )
    );

    const byDay = stats.reduce((group: Record<string, FileStat[]>, value) => {
        (group[value.day] ||= []).push(value);
        return group;
    }, {});

    const totalSize = getSize(stats);

    return { stats, byDay, totalSize, log };
}

export function getFilesSize(list: FileStat[]) {
    return niceBytes(getSize(list));
}

export function niceBytes(value: number) {
    let b = 0;
    for (; 1024 <= value && ++b; ) value /= 1024;
    return (
        value.toFixed(10 > value && 0 < b ? 1 : 0) +
        ' ' +
        ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'][b]
    );
}

export function getSize(list: FileStat[]) {
    return list.reduce((result, item) => {
        return result + item.stat.size;
    }, 0);
}

function log(this: StatObject) {
    const table = new AsciiTable3();
    table.setHeading(t('day'), t('count'));
    table.setStyle('ascii-clean');
    table.addRowMatrix(
        Object.keys(this.byDay)
            .sort()
            .map((day) => [day, this.byDay[day].length])
    );
    const total = t(
        'total',
        this.stats.length.toString(),
        niceBytes(this.totalSize)
    );
    return [table.toString(), total].join('\n');
}
