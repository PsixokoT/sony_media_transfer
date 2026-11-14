import { input } from '@inquirer/prompts';
import AjvModule from 'ajv';
import chalk from 'chalk';
import Conf from 'conf';
import { Options } from './index.js';
import { t } from './translate.js';
import boxen from 'boxen';

const Ajv = AjvModule.default;
const ajv = new Ajv();

const defaults = {
    files: ['DCIM/**/*.{ARW,HIF,JPG}', 'M4ROOT/**/*.{MP4,XML}'],
    ignore: ['**/MEDIAPRO.XML'],
    format: '%Y.%m.%d',
    threads_count: 6,
    batch_size: 0
};

export type ConfigSheme = typeof defaults & { output?: string };
type Property = keyof typeof schema;

const schema = {
    output: {
        type: 'string',
        minLength: 1,
    },
    files: {
        type: 'array',
        uniqueItems: true,
        minItems: 1,
        items: {
            type: 'string',
            minLength: 1,
        },
    },
    ignore: {
        type: 'array',
        uniqueItems: true,
        minItems: 0,
        items: {
            type: 'string',
            minLength: 1,
        },
    },
    format: {
        type: 'string',
    },
    threads_count: {
        type: 'integer',
        minimum: 1,
    },
    batch_size: {
        type: 'integer',
    }
};

class Storage {
    #config;

    constructor() {
        this.#config = new Conf<ConfigSheme>({
            projectName: 'sony-media-transfer',
            schema,
            defaults,
        });
    }

    get output():string {
        return this.#config.get('output') || '/';
    }

    get files() {
        return this.#config.get('files');
    }

    get ignore() {
        return this.#config.get('ignore');
    }

    get format() {
        return this.#config.get('format');
    }

    async init(options: Options) {
        this.checkOptions(options);

        if (options.setup) {
            return this.setup();
        }

        if (!this.#config.get('output')) {
            this.setValue('output', await this.inputString('output'));
        }

        this.log();
    }

    private checkOptions(options: Options) {
        if (options.setup) {
            this.#config.clear();
        }
        (Object.keys(schema) as Array<keyof ConfigSheme>).forEach((prop) => {
            if (options[prop]) {
                const validate = ajv.compile(schema[prop]);
                if (validate(options[prop])) {
                    this.setValue(prop, options[prop]);
                } else {
                    console.log(t('invalide'), prop);
                    delete options[prop];
                }
            }
        });
    }

    private setValue<T>(property: Property, value: T) {
        try {
            this.#config.set(property, value);
        } catch (err) {
            console.log(chalk.red((err as Error).message));
        }
    }

    private async setup() {
        this.setValue('output', await this.inputString('output'));
        this.setValue('files', await this.inputFilePatterns('files'));
        this.setValue('ignore', await this.inputFilePatterns('ignore'));
        this.setValue('format', await this.inputString('format'));
        this.log();
    }

    private async inputString(property: Property) {
        const value = this.#config.get(property);
        return input({
            message: t(['input', property].join('.')) + ':',
            default: value ? String(value) : undefined,
            validate: ajv.compile(schema[property]),
        });
    }

    private async inputFilePatterns(property: 'files' | 'ignore') {
        const defaultValues = this.#config.get(property);
        const result: string[] = [];
        let pattern: string;
        let i = 0;
        const validateResult = ajv.compile(schema[property]);
        const validateItem = ajv.compile(schema[property]['items']);
        const messageText = () => {
            const text = t(['input', property].join('.'));
            if (i >= defaultValues.length && validateResult(result)) {
                return `${text}(${t('input.complete')}):`;
            }
            return text + ':';
        };
        while (
            (pattern = await input({
                message: messageText(),
                default: defaultValues[i++],
                validate: (value) => {
                    const validItem = validateItem(value);
                    if (i < defaultValues.length) {
                        return true;
                    } else {
                        const validResult = validateResult(result);
                        if (
                            validItem &&
                            !validateResult(result.concat(value))
                        ) {
                            return t('input.validate.duplicated');
                        }
                        if (!validItem && !validResult) {
                            return t('input.validate.at_least_one');
                        }
                        return validItem || validResult;
                    }
                },
            })) ||
            !validateResult(result)
        ) {
            if (
                validateItem(pattern) &&
                validateResult(result.concat(pattern))
            ) {
                result.push(pattern);
            }
        }
        return result;
    }

    private toString() {
        const array = (value: string[]) => `[${value.map(item => chalk.green(item)).join(', ')}]`;
        return [
            [chalk.bold(t('storage.output')), chalk.green(this.#config.get('output'))].join(': '),
            [chalk.bold(t('storage.files')), array(this.#config.get('files'))].join(': '),
            [chalk.bold(t('storage.ignore')), array(this.#config.get('ignore'))].join(': '),
            [chalk.bold(t('storage.format')), chalk.green(this.#config.get('format'))].join(': '),
            [chalk.bold(t('storage.threads_count')), chalk.green(this.#config.get('threads_count'))].join(': '),
            [chalk.bold(t('storage.batch_size')), chalk.green(this.#config.get('batch_size'))].join(': ')
        ].join('\n');
    }

    private log() {
        console.log(boxen(this.toString(), { title: t('config'), padding: 1, margin: 1, borderStyle: 'double' }));
    }
}

export const store = new Storage();
