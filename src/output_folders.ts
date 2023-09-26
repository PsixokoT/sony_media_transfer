import { input } from '@inquirer/prompts';
import { join } from 'path';
import { StatObject, getSize } from './files_stat.js';
import { store } from './storage.js';
import { t } from './translate.js';

export type FolderInfo = { day: string; files: string[]; dist: string, totalSize: number };
export async function prepareFiles(
    data: StatObject,
    manual?: boolean,
    perDay?: boolean,
    subfolders?: boolean
): Promise<FolderInfo[]> {
    const days = Object.keys(data.byDay).sort();
    let dist = store.output;
    if (days.length > 1 && !perDay) {
        dist = await getOutputPath(
            dist,
            [days[0], days[days.length - 1]].join(' - '),
            manual
        );
    }

    const folders: Record<string, string> = {};
    for (const day of days) {
        if (days.length > 1 && !subfolders) {
            folders[day] = dist;
        } else {
            folders[day] = await getOutputPath(dist, day, manual);
        }
    }

    return days.map((day) => {
        const totalSize = getSize(data.byDay[day]);
        const files = data.byDay[day].map((file) => file.path);
        dist = folders[day];
        return { day, totalSize, files, dist };
    });
}

async function getOutputPath(dist: string, name: string, manual?: boolean) {
    if (manual) {
        const userInput = await input({
            message: t('input.folder', name) + ':',
        });
        if (userInput) {
            name = [name, userInput].join(' - ');
        }
    }

    return join(dist, name);
}
