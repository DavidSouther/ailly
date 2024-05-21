// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import { basename } from "path";
import { generate } from "./generate.js";
import { LOGGER, resetLogger } from "./settings.js";
import { StatusBarStatusManager } from "./status_manager.js";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
  resetLogger();
  // Use the console to output diagnostic information (console.log) and errors (console.error)
  // This line of code will only be executed once when your extension is activated
  LOGGER.info('Congratulations, your extension "ailly" is now active!', {
    platform: process.platform,
  });

  const statusManager = StatusBarStatusManager.withContext(context);

  // The command has been defined in the package.json file
  // Now provide the implementation of the command with registerCommand
  // The commandId parameter must match the command field in package.json
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ailly.generate",
      async (uri?: vscode.Uri, ..._args) => {
        try {
          if (!uri) {
            uri = vscode.window.activeTextEditor?.document.uri;
          }
          const path = uri?.fsPath ?? "";
          if (!path) {
            return;
          }

          try {
            vscode.window.showInformationMessage(
              `Ailly generating ${basename(path)}`
            );
            await generate(path, { manager: statusManager });
            vscode.window.showInformationMessage(
              `Ailly generated ${basename(path)}`
            );
          } catch (err) {
            vscode.window.showWarningMessage(
              `Ailly failed to generate ${basename(path)}: ${err}`
            );

            LOGGER.error("Failed to generate", { err });
          }
        } catch (err) {
          LOGGER.error("Unknown failure", { err });
        }
      }
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("ailly.edit", async (..._args) => {
      if (!vscode.window.activeTextEditor) {
        return;
      }
      try {
        const uri = vscode.window.activeTextEditor.document.uri;
        const path = uri?.fsPath ?? "";
        if (!path) {
          return;
        }

        const prompt = await vscode.window.showInputBox({
          title: "Prompt",
          prompt: "What edits should Ailly make?",
        });

        if (!prompt) {
          return;
        }

        const start = vscode.window.activeTextEditor.selection.start.line;
        const end = vscode.window.activeTextEditor.selection.end.line;

        try {
          vscode.window.showInformationMessage(
            `Ailly generating ${basename(path)}`
          );
          await generate(path, {
            extensionEdit: { prompt, start, end },
            manager: statusManager,
          });
          vscode.window.showInformationMessage(
            `Ailly edited ${basename(path)}`
          );
        } catch (err) {
          vscode.window.showWarningMessage(
            `Ailly failed to generate ${basename(path)}: ${err}`
          );

          LOGGER.error("Error doing edit", { err });
        }
      } catch (err) {
        LOGGER.error("Unknown error editing", { err });
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("ailly.set-engine", async () => {
      vscode.window.showInformationMessage(`Ailly setting engine`);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("ailly.set-model", async () => {
      vscode.window.showInformationMessage(`Ailly setting model`);
    })
  );
}

// This method is called when your extension is deactivated
export function deactivate() {}
