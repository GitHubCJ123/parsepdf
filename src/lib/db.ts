import Database from "@tauri-apps/plugin-sql";
import { initializeDatabase, type DatabaseInfo } from "@/lib/ipc";

let databaseInfoPromise: Promise<DatabaseInfo> | null = null;
let databasePromise: Promise<Database> | null = null;

export function getDatabaseInfo() {
  databaseInfoPromise ??= initializeDatabase();
  return databaseInfoPromise;
}

export function getDatabase() {
  databasePromise ??= getDatabaseInfo().then((info) => Database.load(info.url));
  return databasePromise;
}

export async function selectRows<TRecord extends Record<string, unknown>>(
  query: string,
  bindValues: unknown[] = [],
) {
  const database = await getDatabase();
  return database.select<TRecord[]>(query, bindValues);
}

export async function execute(query: string, bindValues: unknown[] = []) {
  const database = await getDatabase();
  return database.execute(query, bindValues);
}

export async function getSetting(key: string) {
  const rows = await selectRows<{ value: string }>(
    "SELECT value FROM settings WHERE key = ?1",
    [key],
  );
  return rows[0]?.value ?? null;
}

export async function setSetting(key: string, value: string) {
  await execute("INSERT OR REPLACE INTO settings(key, value) VALUES(?1, ?2)", [key, value]);
}

export async function deleteSetting(key: string) {
  await execute("DELETE FROM settings WHERE key = ?1", [key]);
}
