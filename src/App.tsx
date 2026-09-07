import { useState } from "react";
import { ProcessDetail } from "./components/ProcessDetail";
import { ProcessList } from "./components/ProcessList";
import "./App.css";

function App() {
  const [selectedPid, setSelectedPid] = useState<number | null>(null);

  return (
    <main className="container app-layout">
      <h1>Process Network Inspector</h1>
      <div className="app-layout__panes">
        <section className="app-layout__list">
          <ProcessList selectedPid={selectedPid} onSelectProcess={setSelectedPid} />
        </section>
        <section className="app-layout__detail">
          {selectedPid !== null ? (
            <ProcessDetail pid={selectedPid} />
          ) : (
            <p className="app-layout__placeholder">Select a process to see its connections.</p>
          )}
        </section>
      </div>
    </main>
  );
}

export default App;
