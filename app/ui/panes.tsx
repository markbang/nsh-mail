// Resizable panes with a hairline between them; the line thickens under the
// pointer and while dragged.
import type { JSX } from "nsh";
import { ResizablePanel, ResizablePanelGroup } from "nsh/base";

export function Panes(props: { children?: JSX.Element; label: string; class?: string }) {
  return (
    <ResizablePanelGroup
      class={`flex-1 flex-row ${props.class ?? ""}`}
      handleClass="w-px bg-divider"
      handleAccessibilityLabel={props.label}
    >
      {props.children}
    </ResizablePanelGroup>
  );
}

export function Pane(props: { children?: JSX.Element; size?: number; minSize?: number; maxSize?: number; fixed?: boolean }) {
  return (
    <ResizablePanel size={props.size} minSize={props.minSize} maxSize={props.maxSize} fixed={props.fixed} class="min-w-0">
      {props.children}
    </ResizablePanel>
  );
}
