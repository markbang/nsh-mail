// The search bar over a list: searches as the user types, Escape clears.
import type { InputRef } from "nsh";
import { t } from "nsh/i18n";
import { IconButton, TextField } from "../ui";
import { clearSearch, runSearch, searchQuery, typeSearch } from "../store";

export function SearchField(props: { autoFocus?: boolean; ref?: (ref: InputRef) => void }) {
  return (
    <TextField
      ref={props.ref}
      variant="search"
      value={searchQuery()}
      onChange={typeSearch}
      onSubmit={() => void runSearch(searchQuery())}
      placeholder={t("search.placeholder")}
      label={t("search.label")}
      returnKey="search"
      autoFocus={props.autoFocus}
      leadingIcon="search"
      trailing={
        <If when={searchQuery()}>
          <IconButton icon="close" label={t("search.clear")} size="xs" onPress={clearSearch} class="-me-2" />
        </If>
      }
      testID="search"
    />
  );
}
