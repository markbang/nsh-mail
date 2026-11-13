// The search field over a list: searches as the user types, Escape clears.
import type { InputRef } from "nsh";
import { t } from "nsh/i18n";
import { IconButton, material, TextField } from "../ui";
import { clearSearch, runSearch, searchQuery, typeSearch } from "../store";

export function SearchField(props: { autoFocus?: boolean; ref?: (ref: InputRef) => void }) {
  return (
    <TextField
      ref={props.ref}
      value={searchQuery()}
      onChange={typeSearch}
      onSubmit={() => void runSearch(searchQuery())}
      placeholder={t("search.placeholder")}
      label={t("search.label")}
      returnKey="search"
      autoFocus={props.autoFocus}
      startIcon="search"
      size={material ? "lg" : "sm"}
      radius={material ? "full" : undefined}
      end={
        <If when={searchQuery()}>
          <IconButton icon="close" label={t("search.clear")} size="sm" radius="full" onPress={clearSearch} class={material ? "" : "-me-2 size-6 min-w-0"} />
        </If>
      }
      testID="search"
    />
  );
}
