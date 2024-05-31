// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import path, { basename } from "path";
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
    name: "generate.all",
    manager: statusManager,
    gerund: "running all in",
    pastpart: "ran all in",
    infinitive: "run all in",
    deep: true,
  });

  registerGenerateCommand(context, {
    name: "continue",
    manager: statusManager,
    continued: true,
    gerund: "continuing",
    pastpart: "continued",
  });

  registerGenerateCommand(context, {
    name: "clean",
    manager: statusManager,
    gerund: "cleaning",
    pastpart: "cleaned",
    clean: true,
  });

  registerGenerateCommand(context, {
    name: "clean.all",
    manager: statusManager,
    gerund: "cleaning all in",
    pastpart: "cleaned all in",
    clean: true,
    deep: true,
  });

  registerGenerateCommand(context, {
    name: "edit",
    manager: statusManager,
    gerund: "editing",
    pastpart: "edited",
    edit: true,
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
    deep = false,
    continued = false,
    gerund,
    pastpart,
    infinitive = name,
  }: {
    name: string;
    manager: StatusManager;
    clean?: boolean;
    edit?: boolean;
    deep?: boolean;
    continued?: boolean;
    gerund: string;
    pastpart: string;
    infinitive?: string;
  }
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      `ailly.${name}`,
      async (contextSelection?: vscode.Uri, allSelections?: vscode.Uri[]) => {
        if (edit && !vscode.window.activeTextEditor) {
          return;
        }
        try {
          if (!contextSelection) {
            contextSelection = vscode.window.activeTextEditor?.document.uri;
            allSelections = contextSelection ? [contextSelection] : [];
          }
          let paths = allSelections?.map((path) => path.fsPath) ?? [];
          if (paths.length === 0) {
            return;
          }

          const base = basename(contextSelection?.fsPath ?? paths[0]);

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
            await Promise.all(
              paths.map((path) =>
                generate(path, {
                  manager,
                  clean,
                  depth: deep ? Number.MAX_SAFE_INTEGER : 1,
                  continued,
                  extensionEdit,
                })
              )
            );
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
