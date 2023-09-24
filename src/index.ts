import boxen from 'boxen';
import { Command } from 'commander';
import Listr from 'listr';
import { copyFiles } from './copy.js';
import { getStat } from './files_stat.js';
import { searchUSBDrives } from './search_usb.js';
import { ConfigSheme, store } from './storage.js';
import { t } from './translate.js';

const program = new Command();
program.name('media-transfer');
program.description(t('description'));
program.version('0.0.1');

program.option('-s, --setup', t('option.setup'));
program.option('-m, --manual', t('option.manual'));
program.option('-o, --output <folder-path>', t('option.output'));
program.option('-f, --files <pattern...>', t('option.files'));
program.option('-i, --ignore <pattern...>', t('option.ignore'));
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
            const tasks = await copyFiles(
                stat,
                options.manual,
                options.per_day,
                options.subfolders
            );
            const list = new Listr(tasks);
            await list.run();
        }
    } else {
        console.log('USB drives not found');
    }
}

run(program.opts()).then(() => {
    // console.log('complete');
});
