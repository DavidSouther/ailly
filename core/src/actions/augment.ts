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
        console.log(`Sending ${piece.name} (${piece.path})`);
        await rag.augment(piece);
        console.log(`Completed ${piece.name} (${piece.path})`);
        console.log(piece.augment ?? []);
      } catch (e) {
        console.log(`Error on ${piece.name} (${piece.path})`);
        console.log(e);
      }
      setTimeout(nextPiece, 20);
    };
    nextPiece();
  });
}
