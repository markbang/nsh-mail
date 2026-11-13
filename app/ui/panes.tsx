// Resizable panes 12pt apart on the surface container; M3's drag handle (a
// 4×48pt pill) shows in the spacer under the pointer and grows while dragged.
import type { JSX } from "nsh";
import { ResizablePanel, ResizablePanelGroup } from "nsh/base";

export function Panes(props: { children?: JSX.Element; label: string; class?: string }) {
  return (
    <ResizablePanelGroup
      class={`flex-1 flex-row ${props.class ?? ""}`}
      handleClass="w-px bg-transparent"
      handleThumbClass="h-12 w-1 rounded-full bg-outline opacity-0 group-hover:opacity-100 group-active:h-14 group-active:bg-on-surface group-active:opacity-100 transition-all duration-350 ease-spatial-fast motion-reduce:transition-none"
      handleAccessibilityLabel={props.label}
    >
      {props.children}
    </ResizablePanelGroup>
  );
}

/** A pane; `gap` keeps 6pt to each neighbour, so two panes are 12pt apart. */
export function Pane(props: { children?: JSX.Element; size?: number; minSize?: number; maxSize?: number; fixed?: boolean; first?: boolean; last?: boolean }) {
  return (
    <ResizablePanel size={props.size} minSize={props.minSize} maxSize={props.maxSize} fixed={props.fixed} class={`min-w-0 ${props.first ? "" : "ps-1.5"} ${props.last ? "" : "pe-1.5"}`}>
      {props.children}
    </ResizablePanel>
  );
}
