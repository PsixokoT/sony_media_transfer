import { readdir, rename } from "fs/promises";

readdir(process.cwd()).then(files => {
    files.forEach(f => {
        const name = `_${f}`;
        rename(f, name).then(() => console.log(name));
    })
})