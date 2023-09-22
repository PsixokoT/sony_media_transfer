import { Command } from 'commander';
import fs from 'fs';
import { findUSBDrives } from './find_usb.js';
import { ConfigSheme, store } from './storage.js';
import { t } from './translate.js';

const program = new Command();
program.name('media-transfer');
program.description(t('description'));
program.version('0.0.1');

program.option('-s, --setup', t('option.setup'));
program.option('-m, --manual', t('option.manual'));
program.option('-o, --output <folder-path>', t('option.output'));
program.option('-p, --patterns [pattern...]', t('option.patterns'));
program.option('-i, --ignore [pattern...]', t('option.ignore'));
program.option('-f, --format <format>', t('option.date_format'));
program.option('-t, --threads-count <count>', t('option.threads_count'));
program.option('-b, --batch-size <count>', t('option.batch_size'));
program.option('-d, --per-day', t('option.per_day'));
program.option('-u, --subfolders', t('option.subfolders'));

program.parse(process.argv);

export type Options = ConfigSheme & { setup?: boolean, manual?: boolean };

async function run(options: Options) {
    await store.init(options);
    // console.log(options);

    // findUSBDrives(
    //     ['DCIM/**/*.{ARW,HIF,JPG}', 'PRIVATE/M4ROOT/**/*.{MP4,XML}'],
    //     '**/MEDIAPRO.XML'
    // ).then((result) => {
    //     console.log(result);
    // });
}

run(program.opts()).then(() => {
    console.log('complete');
});
