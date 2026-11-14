import boxen from 'boxen';
import { Command } from 'commander';
import cliProgress from 'cli-progress';
import { FolderInfo, prepareFiles } from './output_folders.js';
import { getStat, niceBytes } from './files_stat.js';
import { searchUSBDrives } from './search_usb.js';
import { ConfigSheme, store } from './storage.js';
import { t } from './translate.js';
import chalk from 'chalk';
import cpy, { ProgressData } from 'cpy';
import { existsSync } from 'fs';
import { join } from 'path';

const program = new Command();
program.name('media-transfer');
program.description(t('description'));
program.version('0.0.1');

program.option('-s, --setup', t('option.setup'));
program.option('-m, --manual', t('option.manual'));
program.option('-o, --output <folder-path>', t('option.output'));
program.option('-f, --files [pattern...]', t('option.files'));
program.option('-i, --ignore [pattern...]', t('option.ignore'));
program.option('-d, --format <date>', t('option.format'));
program.option('-t, --threads_count <count>', t('option.threads_count'));
program.option('-b, --batch_size <count>', t('option.batch_size'));
program.option('-p, --per_day', t('option.per_day'));
program.option('-U, --no-subfolders', t('option.subfolders'));

program.parse(process.argv);

export type Options = ConfigSheme & {
    setup?: boolean;
    manual?: boolean;
    per_day?: boolean;
    subfolders?: boolean;
};

async function run(options: Options) {
    await store.init(options);
    const result = await searchUSBDrives(store.files, store.ignore);
    if (result.length) {
        for (const drive of result) {
            const stat = await getStat(drive.files);
            console.log(
                boxen(stat.log(), {
                    title: t('found_in', drive.cwd),
                    padding: 1,
                    margin: 1,
                    borderStyle: 'double',
                })
            );

            const folders = await prepareFiles(
                stat,
                options.manual,
                options.per_day,
                options.subfolders
            );

            await copy(folders);
        }
    } else {
        console.log('USB drives not found');
    }
}

async function copy(folders: FolderInfo[]) {
    for (const folder of folders) {
        const { files, day, dist, totalSize } = folder;
        const progressBar = new cliProgress.SingleBar({
            format: `${day} -> ${dist} |${chalk.cyan(
                '{bar}'
            )}| {percentage}% || {current}/{totalFiles} Files || Speed: {speed}`,
            barCompleteChar: '\u2588',
            barIncompleteChar: '\u2591',
            hideCursor: true,
        });
        progressBar.start(totalSize, 0, {
            speed: 'N/A',
        });
        await cpy(files, dist, {
            flat: true,
            overwrite: false,
            filter: async (file) => {
                return !existsSync(join(dist, file.name));
            },
        }).on('progress', progressHandler(progressBar, folder));
        progressBar.stop();
    }
}

function progressHandler(bar: cliProgress.SingleBar, folder: FolderInfo) {
    const { totalSize, files } = folder;
    let prev = {
        percent: 0,
        completedFiles: 0,
        completedSize: 0,
        totalFiles: files.length,
    };
    let time = performance.now();
    const startTime = time;
    return function (progress: ProgressData) {
        if (progress.completedSize !== prev.completedSize) {
            const now = performance.now();
            const timeDiff = now - time;
            const bytesDiff = progress.completedSize - prev.completedSize;
            const speed = niceBytes(bytesDiff / timeDiff * 1e3) + '/sec';
            const avgSpeed = niceBytes(progress.completedSize / (now - startTime) * 1e3) + '/sec'
            bar.update(progress.completedSize, { speed: avgSpeed, current: progress.completedFiles, totalFiles: files.length });
            prev = progress;
            time = now;
        }
    };
}

run(program.opts()).then(() => {
    // console.log('complete');
});
