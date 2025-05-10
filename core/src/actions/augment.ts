import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert";
import type { Content } from "../content/content.js";
import { LOGGER } from "../index.js";
import type { RAG } from "../plugin/rag.js";
import { promiseTimeout } from "../util";

export async function augment(content: Content[], rag: RAG): Promise<void> {
  while (content) {
    const piece = assertExists(content.pop());

    try {
      LOGGER.info(`Sending for augment ${piece.name} (${piece.path})`);
      await rag.augment(piece);
      LOGGER.info(`Completed augmenting ${piece.name} (${piece.path})`);
      LOGGER.debug(piece.context.augment ?? []);
    } catch (e) {
      LOGGER.warn(`Error augmenting ${piece.name} (${piece.path})`);
      LOGGER.info(`${e}`);
    }

    await promiseTimeout(20);
  }
}
