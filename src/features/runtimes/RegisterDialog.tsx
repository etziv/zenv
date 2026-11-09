import { open as openPicker } from "@tauri-apps/plugin-dialog";
import { FolderOpenIcon } from "lucide-react";
import { type FormEvent, useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input, NativeSelect } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useRegisterInstallation } from "@/lib/api/queries";
import type { ComponentId, ComponentInfo } from "@/lib/api/types";

export function RegisterDialog({
  open,
  onOpenChange,
  components,
  initialComponent,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  components: ComponentInfo[];
  initialComponent?: ComponentId;
}) {
  const [component, setComponent] = useState<ComponentId | undefined>(initialComponent ?? components[0]?.id);
  const [path, setPath] = useState("");
  const register = useRegisterInstallation();

  const close = (next: boolean) => {
    if (!next) {
      setPath("");
      register.reset();
    }
    onOpenChange(next);
  };

  const browse = async () => {
    const picked = await openPicker({ directory: true, title: "Select the installation folder" });
    if (typeof picked === "string") setPath(picked);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!component || !path.trim()) return;
    register.mutate({ component, path: path.trim() }, { onSuccess: () => close(false) });
  };

  const selected = components.find((c) => c.id === component);

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Register an existing installation</DialogTitle>
            <DialogDescription>
              Zenv runs the program's own version command to verify it, then records where it lives. The files stay
              where they are and are never modified.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-1.5">
            <Label htmlFor="reg-component">Component</Label>
            <NativeSelect
              id="reg-component"
              value={component}
              onChange={(e) => setComponent(e.target.value as ComponentId)}
            >
              {components.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </NativeSelect>
          </div>

          <div className="grid gap-1.5">
            <Label htmlFor="reg-path">Installation folder or executable</Label>
            <div className="flex gap-2">
              <Input
                id="reg-path"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder={selected?.id === "apache" ? "C:\\Apache24" : "C:\\path\\to\\installation"}
                className="font-mono text-[12px]"
                autoFocus
              />
              <Button type="button" variant="outline" onClick={browse}>
                <FolderOpenIcon />
                Browse
              </Button>
            </div>
          </div>

          <ErrorPanel error={register.error} />

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => close(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!path.trim() || register.isPending}>
              {register.isPending ? "Verifying…" : "Register"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
