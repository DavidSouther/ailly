import vscode from "vscode";
import { AillyEdit } from "@ailly/core/src/content/content";

export function insert(edit: AillyEdit, after: string, token: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  const afterArr = after.split("\n");
  editor.edit((builder) => {
    const line = edit.start + afterArr.length - 1;
    const col = afterArr.at(-1)?.length ?? 0;
    builder.insert(new vscode.Position(line, col), token);
  });
}
