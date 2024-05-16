import * as vscode from "vscode";
import { LOGGER } from "./settings.js";
import {
  type AillyEdit,
  isAillyEditReplace,
} from "@ailly/core/lib/content/content";

export function insert(edit: AillyEdit, after: string, token: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  const afterArr = after.split("\n");
  const start = getEditStart(edit);
  return editor.edit(
    (builder) => {
      const line = start + afterArr.length - 1;
      const col = afterArr.at(-1)?.length ?? 0;
      const pos = new vscode.Position(line, col);
      LOGGER.debug(`Insert ${JSON.stringify(token)}`, { pos });
      builder.insert(pos, token);
    },
    { undoStopAfter: false, undoStopBefore: false }
  );
}

function getEditStart(edit: AillyEdit) {
  return isAillyEditReplace(edit) ? edit.start : edit.after + 1;
}

export function deleteEdit(edit: AillyEdit) {
  if (!isAillyEditReplace(edit)) {
    return;
  }
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  return editor.edit(
    (builder) => {
      const start = new vscode.Position(getEditStart(edit), 0);
      const end = new vscode.Position(edit.end, 0);
      const range = new vscode.Range(start, end);
      LOGGER.debug(`Deleting`, { range });
      builder.delete(range);
    },
    { undoStopAfter: false, undoStopBefore: false }
  );
}
export function updateSelection(edit: AillyEdit, newText: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  const newArr = newText.split("\n");
  const startLine = getEditStart(edit);
  const startCol = 0;
  const endLine = startLine + newArr.length - 1;
  const endCol = newArr.at(-1)?.length ?? 0;
  LOGGER.debug(
    `Setting selection to [${startLine}:${startCol}, ${endLine}:${endCol}]`
  );
  editor.selection = new vscode.Selection(startLine, startCol, endLine, endCol);
}
