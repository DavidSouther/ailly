import type { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";
import type { StatusBarItem } from "vscode";
import vscode, { StatusBarAlignment } from "vscode";
import { SETTINGS } from "./settings";

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
        await assertExists(content.responseStream).promise;
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

  private clearStatusBarTimeout: NodeJS.Timeout | undefined;
  updateStatusBarItem() {
    if (this.outstanding === 0) {
      const delay = SETTINGS.getAillyStatusBarHideDelay();
      this.clearStatusBarTimeout = setTimeout(() => {
        this.statusBarItem.hide();
      }, delay);
    } else {
      if (this.clearStatusBarTimeout) {
        clearTimeout(this.clearStatusBarTimeout);
      }
      this.statusBarItem.text = `Ailly: ${this.outstanding}`;
      this.statusBarItem.show();
    }
  }
}

export class MockStatusManager implements StatusManager {
  track(manager: GenerateManager): void {}
}
