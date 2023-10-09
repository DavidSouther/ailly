"use server";

// This state gets reset when next.js reloads the component
let count = 0;

export async function inc() {
  count += 1;
  return count;
}

export async function getCount() {
  return count;
}
