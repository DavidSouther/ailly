// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import { basename } from "path";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
  // Use the console to output diagnostic information (console.log) and errors (console.error)
  // This line of code will only be executed once when your extension is activated
  console.log('Congratulations, your extension "ailly" is now active!');

  // The command has been defined in the package.json file
  // Now provide the implementation of the command with registerCommand
  // The commandId parameter must match the command field in package.json
  let disposable = vscode.commands.registerCommand(
    "ailly.generate",
    async (uri?: vscode.Uri, ..._args) => {
      try {
        let path: string;
        if (uri) {
          path = uri.path;
        } else {
          path = vscode.window.activeTextEditor?.document.uri.fsPath ?? "";
          if (!path) {
            return;
          }
        }

        try {
          const { generate } = await import("./generate.mjs");

          vscode.window.showInformationMessage(
            `Ailly generating ${basename(path)}`
          );
          await generate(path);
          vscode.window.showInformationMessage(
            `Ailly generated ${basename(path)}`
          );
        } catch (e) {
          vscode.window.showWarningMessage(
            `Ailly failed to generate ${basename(path)}: ${e}`
          );

          console.error(e);
        }
      } catch (e) {
        console.error(e);
      }
    }
  );

  context.subscriptions.push(disposable);
}

// This method is called when your extension is deactivated
export function deactivate() {}
