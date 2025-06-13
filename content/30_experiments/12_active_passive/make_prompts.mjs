import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

import { ACTIVE, PASSIVE } from "./prompts.mjs";

const BASE = 'The following sentences are written in either active or passive voice. For each sentence, identify if it is active or passive voice. Repeat the sentence with its bullet point, and then add [ACTIVE] or [PASSIVE] with your answer. If there are multiple clauses, answer [ACTIVE, PASSIVE] or [PASSIVE, ACTIVE] in order.';

/**
 * @param {Iterable<any> | ArrayLike<any>} arr
 */
function shuffle(arr) {
    return Array.from(arr).sort(() => 0.5 - Math.random());
}

/**
 * @param {Iterable<any> | ArrayLike<any>} arr
 * @param {number | undefined} n
 */
function pickRandom(arr, n) {
    return shuffle(arr).slice(0, n);
};

/**
 * @param {number} n
 * @param {string[]} instructions
 */
function getPrompt(n, instructions) {
    const active = pickRandom(ACTIVE, n);
    const passive = pickRandom(PASSIVE, n);
    const sentences = shuffle([...active, ...passive]);

    return [
        BASE,
        ...instructions,
        "",
        ...sentences.map(s => `- ${s}`),
    ].join("\n");
}

// for each model, make a folder model_id & create .aillyrc
// for each {n, instructions, think} matrix, make a folder cnt_n_inst_i
// make K copies in folder
const MODELS = {
    "NovaMicro": "amazon.nova-micro-v1:0",
    "NovaPro": "amazon.nova-pro-v1:0",
    "Claude35Haiku": "us.anthropic.claude-3-5-haiku-20241022-v1:0",
    "Claude37": "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
    "ClaudeSonnet4": "us.anthropic.claude-sonnet-4-20250514-v1:0",
    "ClaudeOpus4": "us.anthropic.claude-opus-4-20250514-v1:0",
    "CohereCommandR": "cohere.command-r-v1:0",
    "CohereCommandRPlus": "cohere.command-r-plus-v1:0",
    "LlamaScout": "meta.llama4-scout-17b-instruct-v1:0",
    "LlamaMaverick": "meta.llama4-maverick-17b-instruct-v1:0",
    "Llama33_70BInstruct": "meta.llama3-3-70b-instruct-v1:0",
    "Mistral7BInstruct": "mistral.mistral-7b-instruct-v0:2",
}

const INSTRUCTIONS = [
    "",
    'Repeat each sentence, and say whether it is "active" or "passive".',

    'The passive voice changes the position of the actor by using the verb to be along with a past participle. Past participles are past tense verb forms that are used as adjectives. For regular verbs, the past participle is the same as the simple past tense, and usually end in -ed, like heated, rotted, or grabbed.',
];

INSTRUCTIONS.push(`${INSTRUCTIONS[1]}\n${INSTRUCTIONS[2]}`);

const THINKING = [
    "",
    "Think about active and passive voice before answering.",
    "Think about each sentence individually before answering."
];
THINKING.push(`${THINKING[1]}\n${THINKING[2]}`);

const N = [3, 6];

function main() {
    for (const [model, id] of Object.entries(MODELS)) {
        const model_system = resolve(`./prompts/${model}/.aillyrc`);
        mkdirSync(dirname(model_system), { recursive: true })
        const head = `---\nmodel: ${id}\n---\n`
        writeFileSync(model_system, head)
        for (let i = 0; i < INSTRUCTIONS.length; i++) {
            const instruction = INSTRUCTIONS[i];
            for (let j = 0; j < THINKING.length; j++) {
                const thought = THINKING[j];
                for (let n = N[0]; n <= N[1]; n++) {
                    for (let k = 0; k < 10; k++) {
                        const path = resolve(`./prompts/${model}/n_${n}_inst_${i}_think_${j}/${k + 10}.md`);
                        mkdirSync(dirname(path), { recursive: true })
                        const prompt = getPrompt(n, [instruction, thought]);
                        writeFileSync(path, prompt);
                    }
                }
            }
        }
    }
}

main();