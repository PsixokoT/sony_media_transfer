import { input } from '@inquirer/prompts';
import { existsSync } from 'fs';
import { mkdir } from 'fs/promises';
import { join } from 'path';
import { setTimeout } from 'timers/promises';
import { FileStat, StatObject } from './files_stat.js';
import { store } from './storage.js';
import { t } from './translate.js';

export async function copyFiles(
    data: StatObject,
    manual?: boolean,
    perDay?: boolean,
    subfolders?: boolean
) {
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

    return data.stats.map((file) => copy(file, folders[file.day]));
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
    const path = join(dist, name);
    if (!existsSync(path)) {
        return mkdir(path, { recursive: true }).then(() => path);
    }
    return path;
}

function copy(file: FileStat, dist: string) {
    return {
        title: file.path + ' -> ' + dist,
        task() {
            return setTimeout(30, () => {
                return file.stat.size;
            });
        },
    };
}
