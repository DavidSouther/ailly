import * as assert from "node:assert";
import { resolve } from "node:path";
import * as vscode from "vscode";
import { deleteEdit, insert } from "./editor.js";
import { getNormalizedText } from "./testing/util.js";

async function activate(docUri: vscode.Uri) {
  const ext = vscode.extensions.getExtension("davidsouther.ailly");
  await ext?.activate();
  const doc = await vscode.workspace.openTextDocument(docUri);
  const editor = await vscode.window.showTextDocument(doc);
  return { doc, editor };
}

suite("Ailly Editor", () => {
  test("delete edit", async () => {
    const path = resolve(__dirname, "..", "testing", "edit.txt");
    const docUri = vscode.Uri.file(path);
    await activate(docUri);

    await deleteEdit({ file: "/ailly/dev", start: 1, end: 3 });

    assert.equal(getNormalizedText(), "Line 1\nLine 4");
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
  });

  test("insert", async () => {
    const path = resolve(__dirname, "..", "testing", "edit.txt");
    const docUri = vscode.Uri.file(path);
    await activate(docUri);

    await insert({ file: "/ailly/dev", start: 1, end: 2 }, "", "Line B\n");

    assert.equal(getNormalizedText(), "Line 1\nLine B\nLine 2\nLine 3\nLine 4");

    await insert({ file: "/ailly/dev", after: 2 }, "Line ", "DEF ");

    assert.equal(
      getNormalizedText(),
      "Line 1\nLine B\nLine 2\nLine DEF 3\nLine 4",
    );
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
  });
});
