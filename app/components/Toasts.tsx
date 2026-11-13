// Where the toast shows: over the bottom of the window, or on phones over the tab's content,
// above the tab bar.
import { View } from "nsh";
import { Toast } from "../ui";
import { toast } from "../store";

export function Toasts(props: { class: string }) {
  return (
    <View class={`absolute inset-x-0 items-center px-4 ${props.class}`} style={{ pointerEvents: "box-none" }}>
      <Each of={toast() ? [toast()!] : []} key={(x) => x.id}>
        {(x) => <Toast text={x().text} tone={x().tone} action={x().action} />}
      </Each>
    </View>
  );
}
