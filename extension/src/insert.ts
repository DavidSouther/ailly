import vscode from "vscode";
import {
  isAillyEditReplace,
  type AillyEdit,
} from "@ailly/core/lib/content/content.js";

export function insert(edit: AillyEdit, after: string, token: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  const afterArr = after.split("\n");
  const start =
    (isAillyEditReplace(edit) ? edit.start : edit.after) + afterArr.length - 1;
  editor.edit((builder) => {
    const line = start;
    const col = afterArr.at(-1)?.length ?? 0;
    builder.insert(new vscode.Position(line, col), token);
  });
}
