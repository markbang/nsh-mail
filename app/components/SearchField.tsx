// The search field over a list: searches as the user types, Escape clears.
import { View, type InputRef } from "nsh";
import { t } from "nsh/i18n";
import { Icon, IconButton, TextField } from "../ui";
import { clearSearch, runSearch, searchQuery, typeSearch } from "../store";

export function SearchField(props: { autoFocus?: boolean; ref?: (ref: InputRef) => void }) {
  return (
    <View class="flex-row items-center gap-2">
      <View class="flex-1 justify-center">
        <TextField
          ref={props.ref}
          value={searchQuery()}
          onChange={typeSearch}
          onSubmit={() => void runSearch(searchQuery())}
          placeholder={t("search.placeholder")}
          label={t("search.label")}
          returnKey="search"
          autoFocus={props.autoFocus}
          class="ps-8"
          testID="search"
        />
        <View class="absolute start-2.5" style={{ pointerEvents: "none" }}>
          <Icon name="search" size="sm" tone="muted-foreground" />
        </View>
      </View>
      <If when={searchQuery()}>
        <IconButton icon="close" label={t("search.clear")} size="sm" onPress={clearSearch} />
      </If>
    </View>
  );
}
