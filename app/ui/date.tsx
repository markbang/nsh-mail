// M3 date and time inputs on nsh/base: the docked date picker's month of days
// (40pt round days, the selected one filled with primary, today ringed) and a
// time field drawn as an outlined text field. Names of months and weekdays,
// the first day of the week and 12/24-hour time follow the user's locale.
import { locale, localeSettings, Text, View } from "nsh";
import { Calendar as BaseCalendar, TimeField as BaseTimeField, type CalendarEntry } from "nsh/base";
import { Icon } from "./icon";
import { focusRing } from "./tokens";

const monthName = (year: number, month: number) => new Intl.DateTimeFormat(locale(), { month: "long" }).format(new Date(year, month - 1, 1));
// 2023-01-01 was a Sunday: day `n` of the week is the 1st plus n.
const weekdayName = (day: number) => new Intl.DateTimeFormat(locale(), { weekday: "narrow" }).format(new Date(2023, 0, 1 + day));

function CalendarItem(props: { item: CalendarEntry }) {
  const item = props.item;
  switch (item.kind) {
    case "previous":
      return <Icon name="back" size="sm" tone="on-surface-variant" />;
    case "next":
      return <Icon name="chevronRight" size="sm" tone="on-surface-variant" />;
    case "weekday":
      return <Text class="text-body-large text-on-surface">{weekdayName(item.value)}</Text>;
    case "monthToggle":
    case "month":
      return <Text class="text-label-large font-medium text-on-surface-variant group-selected:text-on-primary">{monthName(item.year || 2000, item.value || item.month)}</Text>;
    case "yearToggle":
    case "year":
      return <Text class="text-label-large font-medium text-on-surface-variant group-selected:text-on-primary">{String(item.year || item.value)}</Text>;
    default:
      return (
        <Text class="text-body-large tabular-nums text-on-surface group-selected:text-on-primary group-data-today:text-primary group-selected:group-data-today:text-on-primary group-disabled:text-on-surface/38">
          {String(item.value)}
        </Text>
      );
  }
}

/** A month of days to pick one date (`YYYY-MM-DD`); `min` and earlier days cannot be picked. */
export function DateCalendar(props: { value: string; onChange: (date: string) => void; min?: string; label: string; previousLabel: string; nextLabel: string; testID?: string }) {
  return (
    <View accessibilityLabel={props.label} testID={props.testID}>
      <BaseCalendar
        value={props.value}
        onChange={({ value }) => typeof value === "string" && props.onChange(value)}
        disabledMatcher={props.min ? { before: props.min } : undefined}
        firstDayOfWeek={localeSettings().firstDayOfWeek % 7}
        previousAccessibilityLabel={props.previousLabel}
        nextAccessibilityLabel={props.nextLabel}
        class="gap-1"
        headerClass="h-12 flex-row items-center justify-between"
        rowClass="flex-row justify-between"
        itemClass={`group h-10 min-w-10 items-center justify-center rounded-full px-1 hover:bg-on-surface/8 active:bg-on-surface/10 selected:bg-primary data-today:border data-today:border-primary selected:data-today:border-0 disabled:bg-transparent data-outside:opacity-0 transition-colors duration-150 ease-effects-fast motion-reduce:transition-none ${focusRing}`}
        renderItem={(item) => <CalendarItem item={item} />}
      />
    </View>
  );
}

/** A time of day (`HH:MM`) edited segment by segment, in an outlined field. */
export function TimeInput(props: { value: string; onChange: (time: string) => void; label: string; testID?: string }) {
  const h12 = () => localeSettings().hourCycle === "h11" || localeSettings().hourCycle === "h12";
  return (
    <BaseTimeField
      value={props.value}
      onChange={({ value }) => props.onChange(value)}
      hourCycle={h12() ? "h12" : "h23"}
      accessibilityLabel={props.label}
      testID={props.testID}
      class="h-14 flex-row items-center gap-0.5 self-start rounded-extra-small border border-outline px-4 hover:border-on-surface focus-within:border-2 focus-within:border-primary"
      segmentClass="rounded-extra-small px-1 py-0.5 text-body-large tabular-nums text-on-surface focus:bg-primary-container focus:text-on-primary-container"
      separatorClass="text-body-large text-on-surface-variant"
    />
  );
}
