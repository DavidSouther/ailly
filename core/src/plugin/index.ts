export interface Message {
  role: "system" | "user" | "assistant";
  content: string;
  tokens?: number;
}

export interface Summary {
  tokens: number;
  prompts: number;
}
