import { MonitorIcon, MoonIcon, SunIcon } from "lucide-react";

import { Card } from "@/components/ui/card";
import { setTheme, type Theme, useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

const OPTIONS: { value: Theme; label: string; icon: typeof SunIcon }[] = [
  { value: "system", label: "Follow Windows", icon: MonitorIcon },
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
];

export function ThemePicker() {
  const theme = useTheme();
  return (
    <Card className="flex flex-wrap items-center gap-3 p-4">
      <p className="flex-1 text-sm">Theme</p>
      <div role="radiogroup" aria-label="Theme" className="flex gap-1 rounded-md border bg-muted/40 p-0.5">
        {OPTIONS.map(({ value, label, icon: Icon }) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={theme === value}
            onClick={() => setTheme(value)}
            className={cn(
              "flex items-center gap-1.5 rounded-sm px-3 py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
              theme === value ? "bg-card text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon className="size-3.5" /> {label}
          </button>
        ))}
      </div>
    </Card>
  );
}
