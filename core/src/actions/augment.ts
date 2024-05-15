import { LOGGER } from "../ailly.js";
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
        LOGGER.info(`Sending for augment ${piece.name} (${piece.path})`);
        await rag.augment(piece);
        LOGGER.info(`Completed augmenting ${piece.name} (${piece.path})`);
        LOGGER.debug(piece.context.augment ?? []);
      } catch (e) {
        LOGGER.warn(`Error augmenting ${piece.name} (${piece.path})`);
        LOGGER.info(`${e}`);
      }
      setTimeout(nextPiece, 20);
    };
    nextPiece();
  });
}
