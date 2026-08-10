import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface AppCommands {
  app_ready: string;
}

export async function appReady(): Promise<AppCommands["app_ready"]> {
  return invoke("app_ready");
}

export async function chooseSessionsDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    recursive: true,
  });
  return typeof selected === "string" ? selected : null;
}
