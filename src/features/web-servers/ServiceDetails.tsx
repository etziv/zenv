import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { LogView } from "@/components/log-view";
import { Loading } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useLogSources, useServiceEvents, useServiceLog } from "@/lib/api/queries";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";

function Events({ id }: { id: string }) {
  const events = useServiceEvents(id);
  if (events.isPending) return <Loading />;
  if (events.error) return <ErrorPanel error={events.error} />;
  if (events.data.length === 0) return <p className="text-xs text-muted-foreground">No activity yet.</p>;
  return (
    <ol className="max-h-72 space-y-1 overflow-auto">
      {events.data.map((e) => (
        <li key={e.id} className="flex gap-3 text-xs">
          <span className="w-16 shrink-0 text-muted-foreground" title={e.createdAt}>
            {formatRelative(e.createdAt)}
          </span>
          <span
            className={cn(
              "min-w-0 break-words",
              e.level === "error" && "text-destructive",
              e.level === "warning" && "text-warning",
            )}
          >
            {e.message}
          </span>
        </li>
      ))}
    </ol>
  );
}

function Logs({ id }: { id: string }) {
  const sources = useLogSources(id);
  const [selected, setSelected] = useState<string>("error");
  const [follow, setFollow] = useState(true);
  const log = useServiceLog(id, selected, follow);

  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-1">
        {sources.data?.map((s) => (
          <Button
            key={s.id}
            size="sm"
            variant={s.id === selected ? "secondary" : "ghost"}
            onClick={() => setSelected(s.id)}
          >
            {s.label}
          </Button>
        ))}
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} className="accent-primary" />
          Follow
        </label>
      </div>
      {log.isPending ? <Loading /> : log.error ? <ErrorPanel error={log.error} /> : <LogView tail={log.data} follow={follow} />}
    </div>
  );
}

export function ServiceDetails({ id }: { id: string }) {
  return (
    <Tabs defaultValue="logs" className="px-4 pb-4">
      <TabsList>
        <TabsTrigger value="logs">Logs</TabsTrigger>
        <TabsTrigger value="events">Activity</TabsTrigger>
      </TabsList>
      <TabsContent value="logs">
        <Logs id={id} />
      </TabsContent>
      <TabsContent value="events">
        <Events id={id} />
      </TabsContent>
    </Tabs>
  );
}
