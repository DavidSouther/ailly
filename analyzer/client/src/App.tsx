import { FolderOpen, Search } from "lucide-react";
import { useEffect, useState } from "react";

import { appReady, chooseSessionsDirectory } from "./tauri";

export function App() {
  const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void appReady().catch(() => {
      setError("The desktop shell is not available.");
    });
  }, []);

  async function handleOpenSessions() {
    setError(null);
    try {
      setSelectedDirectory(await chooseSessionsDirectory());
    } catch {
      setError("Could not open the sessions directory picker.");
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
      <section className="flex max-w-lg flex-col items-center gap-5 text-center">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-background-active text-brand">
          <Search size={30} strokeWidth={1.8} />
        </div>
        <div className="flex flex-col gap-2">
          <p className="text-xs font-semibold uppercase tracking-[0.18em] text-foreground-muted">
            Ailly Analyzer
          </p>
          <h1 className="text-2xl font-semibold tracking-tight">
            Investigate completed agent sessions
          </h1>
          <p className="text-sm leading-6 text-foreground-muted">
            Open a local session directory to see where an agent went wrong and where the budget
            went. Your source transcripts stay on this device.
          </p>
        </div>
        <button
          className="flex items-center gap-2 rounded-lg bg-foreground px-4 py-2.5 text-sm font-medium text-background transition-opacity hover:opacity-85"
          type="button"
          onClick={handleOpenSessions}
        >
          <FolderOpen size={16} />
          Open sessions
        </button>
        {selectedDirectory && (
          <p className="text-xs text-foreground-muted">Selected: {selectedDirectory}</p>
        )}
        {error && <p className="text-xs text-red-600">{error}</p>}
      </section>
    </main>
  );
}
