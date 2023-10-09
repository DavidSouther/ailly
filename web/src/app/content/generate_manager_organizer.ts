import { GenerateManager } from "@ailly/core/src/ailly";
import { Content } from "@ailly/core/src/content";

interface GenerateManagerOrganizer {
  start(content: Content[]): Promise<[string, GenerateManager]>;
  get(id: string): GenerateManager | undefined;
}

const ID_CHARS =
  "0123456789abcdefghijklmnopqrstuvwzyzABCDEFGHIJKLMNOPQRSTUVWZYZ";
const getChar = () => ID_CHARS.at(Math.floor(Math.random() * ID_CHARS.length));
const makeId = () => new Array(6).fill(undefined).map(getChar).join("");
const findId = (map: Map<string, unknown>) => {
  let id;
  do {
    id = makeId();
  } while (map.has(id));
  return id;
};

export function makeGenerateManagerOrganizer(): GenerateManagerOrganizer {
  const organizer = new Map<string, GenerateManager>();

  return {
    async start(content) {
      let manager = await GenerateManager.from(content, {});
      let id = findId(organizer);
      organizer.set(id, manager);
      return [id, manager];
    },
    get(id) {
      return organizer.get(id);
    },
  };
}
