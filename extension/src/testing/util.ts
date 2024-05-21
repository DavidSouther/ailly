import * as vscode from "vscode";
import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";

export function getNormalizedText() {
  const activeWindow: vscode.TextEditor = assertExists(
    vscode.window.activeTextEditor
  );
  const currentText = activeWindow.document.getText();
  return currentText.replaceAll("\r", "");
}
