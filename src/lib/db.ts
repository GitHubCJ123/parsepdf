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
