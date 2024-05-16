import * as assert from "assert";
import { afterEach } from "mocha";
import { resolve } from "path";
import * as vscode from "vscode";
import { deleteEdit, insert, updateSelection } from "./editor.js";
const { assertExists } = require("@davidsouther/jiffies/lib/cjs/assert.js");

async function activate(docUri: vscode.Uri) {
  const ext = vscode.extensions.getExtension("davidsouther.ailly");
  await ext?.activate();
  const doc = await vscode.workspace.openTextDocument(docUri);
  const editor = await vscode.window.showTextDocument(doc);
  return { doc, editor };
}

suite("Ailly Editor", () => {
  afterEach(() =>
    vscode.commands.executeCommand("workbench.action.closeActiveEditor")
  );

  test("delete edit", async () => {
    const path = resolve(__dirname, "..", "testing", "edit.txt");
    const docUri = vscode.Uri.file(path);
    await activate(docUri);

    await deleteEdit({ file: "/ailly/dev", start: 1, end: 3 });

    const activeWindow = assertExists(vscode.window.activeTextEditor);
    assert.equal(activeWindow.document.getText(), "Line 1\nLine 4");
  });

  test("insert", async () => {
    const path = resolve(__dirname, "..", "testing", "edit.txt");
    const docUri = vscode.Uri.file(path);
    await activate(docUri);

    await insert({ file: "/ailly/dev", start: 1, end: 2 }, "", "Line B\n");

    const activeWindow = assertExists(vscode.window.activeTextEditor);
    assert.equal(
      activeWindow.document.getText(),
      "Line 1\nLine B\nLine 2\nLine 3\nLine 4"
    );

    await insert({ file: "/ailly/dev", after: 2 }, "Line ", "DEF ");

    assert.equal(
      activeWindow.document.getText(),
      "Line 1\nLine B\nLine 2\nLine DEF 3\nLine 4"
    );
  });
});
