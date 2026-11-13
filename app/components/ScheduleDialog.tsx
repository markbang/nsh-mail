// Schedule send's own date and time: a month of days and a time field, in the
// user's time zone, refusing a time that has passed.
import { signal, Text, View } from "nsh";
import { t } from "nsh/i18n";
import { Button, DateCalendar, Dialog, DialogBody, DialogFooter, TimeInput } from "../ui";
import { sendDate } from "../lib/format";

const pad = (n: number) => String(n).padStart(2, "0");
const isoDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
// Local midnight of an ISO date, then the time of day: the user's zone throughout.
const localTime = (date: string, time: string) => {
  const [y, m, d] = date.split("-").map(Number);
  const [h, min] = time.split(":").map(Number);
  return new Date(y, m - 1, d, h, min).getTime();
};

export function ScheduleDialog(props: { open: boolean; onClose: () => void; onPick: (at: number) => void }) {
  const tomorrow = new Date();
  tomorrow.setDate(tomorrow.getDate() + 1);
  const date = signal(isoDate(tomorrow));
  const time = signal("08:00");
  const at = () => localTime(date(), time());
  const past = () => at() <= Date.now();
  return (
    <Dialog open={props.open} onClose={props.onClose} label={t("compose.schedule")} title={t("compose.schedule")} size="sm" testID="schedule-dialog">
      <DialogBody class="gap-3">
        <DateCalendar
          value={date()}
          onChange={(d) => date.set(d)}
          min={isoDate(new Date())}
          label={t("schedule.date")}
          previousLabel={t("schedule.previousMonth")}
          nextLabel={t("schedule.nextMonth")}
          testID="schedule-date"
        />
        <View class="flex-row items-center gap-4">
          <Text class="text-body-large tracking-body-large text-on-surface-variant">{t("schedule.time")}</Text>
          <TimeInput value={time()} onChange={(v) => time.set(v)} label={t("schedule.time")} testID="schedule-time" />
        </View>
        <Text class={`text-body-medium tracking-body-medium ${past() ? "text-error" : "text-on-surface-variant"}`} testID="schedule-summary">
          {past() ? t("schedule.past") : t("outbox.sends", { when: sendDate(at()) })}
        </Text>
      </DialogBody>
      <DialogFooter>
        <Button variant="text" label={t("common.cancel")} onPress={props.onClose} />
        <Button variant="text" icon="clock" label={t("compose.schedule")} disabled={past()} onPress={() => props.onPick(at())} testID="schedule-confirm" />
      </DialogFooter>
    </Dialog>
  );
}
