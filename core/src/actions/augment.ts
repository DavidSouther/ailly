import { DEFAULT_LOGGER } from "@davidsouther/jiffies/lib/esm/log.js";
import { Content } from "../content/content.js";
import { RAG } from "../plugin/rag.js";

export async function augment(content: Content[], rag: RAG): Promise<void> {
  const _content = [...content];
  return new Promise(async (resolve, reject) => {
    const nextPiece = async () => {
      const piece = _content.pop()!;
      if (!piece) {
        return resolve();
      }
      try {
        DEFAULT_LOGGER.info(`Sending ${piece.name} (${piece.path})`);
        await rag.augment(piece);
        DEFAULT_LOGGER.info(`Completed ${piece.name} (${piece.path})`);
        DEFAULT_LOGGER.info(piece.augment ?? []);
      } catch (e) {
        DEFAULT_LOGGER.info(`Error on ${piece.name} (${piece.path})`);
        DEFAULT_LOGGER.info(`${e}`);
      }
      setTimeout(nextPiece, 20);
    };
    nextPiece();
  });
}
