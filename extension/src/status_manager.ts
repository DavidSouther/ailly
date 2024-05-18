import type { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import type { StatusBarItem } from "vscode";
import vscode, { StatusBarAlignment } from "vscode";

export interface StatusManager {
  track(manager: GenerateManager): void;
}

export class StatusBarStatusManager implements StatusManager {
  outstanding: number = 0;
  statusBarItem: StatusBarItem;

  static withContext({ subscriptions }: vscode.ExtensionContext) {
    const statusManager = new StatusBarStatusManager();
    subscriptions.push(statusManager.statusBarItem);
    return statusManager;
  }

  private constructor() {
    this.statusBarItem = vscode.window.createStatusBarItem(
      "ailly.statusBarItem",
      StatusBarAlignment.Right
    );
    this.updateStatusBarItem();
  }

  track(manager: GenerateManager) {
    manager.threads.forEach((thread) =>
      thread.forEach(async (content) => {
        this.addOutstanding();
        await content.responseStream.promise;
        this.finishOutstanding();
      })
    );
  }

  addOutstanding(count: number = 1) {
    this.outstanding += count;
    this.updateStatusBarItem();
  }
  finishOutstanding() {
    this.outstanding -= 1;
    this.updateStatusBarItem();
  }

  updateStatusBarItem() {
    this.statusBarItem.text = `Ailly: ${this.outstanding}`;
    this.statusBarItem.show();
  }
}

export class MockStatusManager implements StatusManager {
  track(manager: GenerateManager): void {}
}
