import { Content } from "../content/content";
import { RAG } from "../rag";

export async function updateDatabase(
  content: Content[],
  rag: RAG
): Promise<void> {
  const _content = [...content];
  return new Promise(async (resolve, reject) => {
    const nextPiece = async () => {
      const piece = _content.pop()!;
      if (!piece) {
        return resolve();
      }
      try {
        console.log(`Sending ${piece.name} (${piece.path})`);
        await rag.add(piece);
        console.log(`Completed ${piece.name} (${piece.path})`);
      } catch (e) {
        console.log(`Error on ${piece.name} (${piece.path})`);
        console.log(e);
      }
      setTimeout(nextPiece, 20);
    };
    nextPiece();
  });
}
