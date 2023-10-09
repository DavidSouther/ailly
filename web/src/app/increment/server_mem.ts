"use server";

// Using a persistent memory store

import { MemBackedStore } from "../../store";

let client: MemBackedStore | undefined;
async function getClient() {
  if (!client) client = await MemBackedStore.open();
  return client;
}

export async function incStore() {
  (await getClient()).set("count", `${(await getStore()) + 1}`);
  return getStore();
}

export async function getStore() {
  return Number((await getClient()).get("count") ?? "0");
}

export async function closeStore() {
  getClient().then((c) => c.close());
}
export async function compactStore() {
  getClient().then((c) => c.compact());
}
