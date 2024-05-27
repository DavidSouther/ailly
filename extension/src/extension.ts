// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import { basename } from "path";
import { generate, type ExtensionEdit } from "./generate.js";
import { LOGGER, resetLogger } from "./settings.js";
import {
  StatusBarStatusManager,
  type StatusManager,
} from "./status_manager.js";

export function activate(context: vscode.ExtensionContext) {
  resetLogger();

  LOGGER.info('Congratulations, your extension "ailly" is now active!', {
    platform: process.platform,
  });

  const statusManager = StatusBarStatusManager.withContext(context);

  registerGenerateCommand(context, {
    name: "generate",
    manager: statusManager,
    gerund: "running",
    pastpart: "ran",
    infinitive: "run",
  });

  registerGenerateCommand(context, {
    name: "edit",
    manager: statusManager,
    gerund: "editing",
    pastpart: "edited",
    edit: true,
  });

  registerGenerateCommand(context, {
    name: "clean",
    manager: statusManager,
    gerund: "cleaning",
    pastpart: "cleaned",
    clean: true,
  });

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

export function registerGenerateCommand(
  context: vscode.ExtensionContext,
  {
    name,
    manager,
    edit = false,
    clean = false,
    gerund,
    pastpart,
    infinitive = name,
  }: {
    name: string;
    manager: StatusManager;
    clean?: boolean;
    edit?: boolean;
    gerund: string;
    pastpart: string;
    infinitive?: string;
  }
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      `ailly.${name}`,
      async (uri?: vscode.Uri, ..._args) => {
        if (edit && !vscode.window.activeTextEditor) {
          return;
        }
        try {
          if (!uri) {
            uri = vscode.window.activeTextEditor?.document.uri;
          }
          const path = uri?.fsPath ?? "";
          if (!path) {
            return;
          }

          const base = basename(path);

          let extensionEdit: ExtensionEdit | undefined;
          if (edit) {
            const prompt = await vscode.window.showInputBox({
              title: "Prompt",
              prompt: "What edits should Ailly make?",
            });

            if (!prompt) {
              return;
            }

            const start = vscode.window.activeTextEditor!.selection.start.line;
            const end = vscode.window.activeTextEditor!.selection.end.line;
            extensionEdit = { prompt, start, end };
          }

          try {
            vscode.window.showInformationMessage(`Ailly ${gerund} ${base}`);
            await generate(path, {
              manager,
              clean,
              extensionEdit,
            });
            vscode.window.showInformationMessage(`Ailly ${pastpart} ${base}`);
          } catch (err) {
            vscode.window.showWarningMessage(
              `Ailly failed to ${infinitive} ${base}`
            );

            LOGGER.error(`Failed to ${infinitive}`, { err });
          }
        } catch (err) {
          LOGGER.error("Unknown failure", { err });
        }
      }
    )
  );
}
