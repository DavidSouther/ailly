import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { LEVEL } from "@davidsouther/jiffies/lib/esm/log.js";
import { GitignoreFs, LOGGER } from "@ailly/core";
import { loadContent } from "@ailly/core/lib/content/content.js";

import { ACTIVE, ALL, COMPOSITE, COMPOSITE_AP, COMPOSITE_PA, PASSIVE } from "./prompts.mjs";

/**
 * @param {import("@ailly/core/lib/engine/bedrock/bedrock.js").BedrockDebug} debug
 */
function stats(debug = { id: 'missing' }) {
    const { statistics = {} } = debug;
    const { firstByteLatency, invocationLatency } = statistics;
    const duration = (firstByteLatency && invocationLatency) ? (Number(invocationLatency) - Number(firstByteLatency)).toFixed(0) : -1;

    return {
        duration,
        tokens: statistics.outputTokenCount ?? -1,
    }
}

const fs = new GitignoreFs(new NodeFileSystemAdapter());
fs.cd(process.cwd())
fs.cd("prompts")
LOGGER.level = LEVEL.WARN;
const LINE_MATCHER = /- (?<sentence>.+\.) \[(?<identified>[^\]]+)\]/;
const contents = await loadContent(fs, {}, 10);

process.stdout.write("id,model,modelId,inst,think,expected,right,partial,wrong,error,check,duration,tokens\n");

const INCORRECT = [];
const BNS = [0, 0];

for (const content of Object.values(contents)) {
    const { id, model, n, inst, think } = content.path.match(/\/(?<id>(?<model>[^\/]+)\/n_(?<n>\d)_inst_(?<inst>\d)_think_(?<think>\d)\/1\d.md)/)?.groups ?? {};
    if (!content.response) {
        continue;
    }
    const lines = content.response.split("\n");
    const modelId = content.meta?.debug?.model ?? 'unknown';
    const expected = Number(n) * 2;
    let right = 0;
    let partial = 0;
    let wrong = 0;
    let error = 0;
    const { duration, tokens } = stats(content.meta?.debug)
    for (const line of lines) {
        const match = line.match(LINE_MATCHER);
        if (match) {
            const { sentence, identified } = match.groups ?? {};
            if (sentence === "Butternut squash, a favorite among gourd enthusiasts, can be roasted, grilled, or even fried.") {
                if (identified === "PASSIVE") {
                    BNS[0] += 1;
                } else {
                    BNS[1] += 1;
                }
            }
            if (identified === 'ACTIVE' || identified === 'ACTIVE, ACTIVE') {
                if (COMPOSITE.has(sentence)) {
                    partial += 1;
                } else if (ACTIVE.has(sentence)) {
                    right += 1;
                } else if (PASSIVE.has(sentence)) {
                    wrong += 1;
                    INCORRECT.push([sentence, identified, model]);
                } else {
                    error += 1;
                }
            } else if (identified === 'PASSIVE' || identified === 'PASSIVE, PASSIVE') {
                if (COMPOSITE.has(sentence)) {
                    partial += 1;
                } else if (PASSIVE.has(sentence)) {
                    right += 1;
                } else if (ACTIVE.has(sentence)) {
                    wrong += 1;
                    INCORRECT.push([sentence, identified, model]);
                } else {
                    error += 1;
                }
            } else if (identified === 'ACTIVE, PASSIVE') {
                if (COMPOSITE_AP.has(sentence)) {
                    right += 1;
                } else if (ALL.has(sentence)) {
                    wrong += 1;
                    INCORRECT.push([sentence, identified, model]);
                } else {
                    error += 1;
                }
            } else if (identified === 'PASSIVE, ACTIVE') {
                if (COMPOSITE_PA.has(sentence)) {
                    right += 1;
                } else if (ALL.has(sentence)) {
                    wrong += 1;
                    INCORRECT.push([sentence, identified, model]);
                } else {
                    error += 1;
                }
            }
        }
    }
    const check = right + wrong + partial + error;
    process.stdout.write([
        id,
        model,
        modelId,
        inst,
        think,
        expected,
        right,
        partial,
        wrong,
        error,
        check,
        duration,
        tokens
    ].join(","));
    process.stdout.write("\n")
}
// process.stdout.write("identified,sentence,model\n");
// for (const [id, s, m] of INCORRECT) {
//     process.stdout.write(`${id},"${s}",${m}\n`)
// }
// console.log({ right: BNS[0], wrong: BNS[1], total: BNS[0] + BNS[1] });