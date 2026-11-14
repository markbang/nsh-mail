"""Seeds GreenMail (auth disabled) with three accounts' fixed mailboxes for the screenshots: SMTP to deliver, IMAP to set flags."""
import imaplib, io, os, smtplib, struct, zipfile, zlib
from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
from email.utils import format_datetime

WORK = "lin.chen@northwind.dev"
HOME = "lin@chen-family.example"
CN = "chenlin@mail.example"
CST = timezone(timedelta(hours=8))
# The browser's clock is frozen at NOW too (shoot.mjs), so "today" and "yesterday" never drift.
NOW = datetime(2026, 11, 13, 9, 30, tzinfo=CST)
SMTP = int(os.environ.get("GREENMAIL_SMTP_PORT", "3025"))
IMAP = int(os.environ.get("GREENMAIL_IMAP_PORT", "3143"))


def pdf(lines):
    text = "BT /F1 14 Tf 72 760 Td 18 TL " + " ".join(f"({l}) Tj T*" for l in lines) + " ET"
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        f"<< /Length {len(text)} >>\nstream\n{text}\nendstream",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out, offs = b"%PDF-1.4\n", []
    for i, o in enumerate(objs, 1):
        offs.append(len(out))
        out += f"{i} 0 obj\n{o}\nendobj\n".encode()
    xref = len(out)
    out += f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode()
    out += b"".join(f"{o:010d} 00000 n \n".encode() for o in offs)
    out += f"trailer\n<< /Size {len(objs)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    return out


def png(w, h, c1, c2):
    rows = b""
    for y in range(h):
        t = y / (h - 1)
        px = bytes(int(a + (b - a) * t) for a, b in zip(c1, c2))
        rows += b"\0" + px * w
    def chunk(k, d):
        return struct.pack(">I", len(d)) + k + d + struct.pack(">I", zlib.crc32(k + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def xlsx(rows):
    def cell(v):
        return f'<c t="inlineStr"><is><t>{v}</t></is></c>' if isinstance(v, str) else f"<c><v>{v}</v></c>"
    sheet = "".join(f'<row r="{i}">' + "".join(cell(v) for v in r) + "</row>" for i, r in enumerate(rows, 1))
    b = io.BytesIO()
    with zipfile.ZipFile(b, "w") as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
        z.writestr("_rels/.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        z.writestr("xl/workbook.xml", '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/worksheets/sheet1.xml", f'<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{sheet}</sheetData></worksheet>')
    return b.getvalue()


ICS = """BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Northwind//Calendar//EN\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:design-sync-1114@northwind.dev\r\nDTSTART:20261114T020000Z\r\nDTEND:20261114T024500Z\r\nSUMMARY:Design sync\r\nLOCATION:Room 4B / Meet\r\nORGANIZER:mailto:maya@northwind.dev\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"""

PDF = "application/pdf"
PNG = "image/png"
XLSX = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"

def html_card(title, body, accent="#0b6bcb", footer=""):
    return f"""<!doctype html><html><body style="margin:0;background:#f4f5f7;font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif">
<table width="100%" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding:24px">
<table width="600" cellpadding="0" cellspacing="0" style="background:#fff;border-radius:8px;overflow:hidden">
<tr><td style="background:{accent};color:#fff;padding:20px 28px;font-size:20px;font-weight:600">{title}</td></tr>
<tr><td style="padding:24px 28px;color:#222;font-size:15px;line-height:1.55">{body}</td></tr>
<tr><td style="padding:16px 28px;color:#888;font-size:12px;border-top:1px solid #eee">{footer}</td></tr>
</table></td></tr></table></body></html>"""

# (key, to, from, subject, days_ago, hour, text, html, attachments, flags, reply_to_key)
M = [
    ("w-gh-pr", WORK, ("GitHub", "notifications@github.com"), "[northwind/atlas] feat(app): conversation view (PR #12)", 13, 10,
     "@jonas-weber requested your review on this pull request.\n\nAdds a threaded conversation view to the reader.\n\nView it on GitHub: https://github.com/northwind/atlas/pull/12",
     html_card("Review requested", "<p><b>@jonas-weber</b> requested your review on <a href='https://github.com/northwind/atlas/pull/12'>#12 feat(app): conversation view</a>.</p><p>Adds a threaded conversation view to the reader, grouping replies by <code>References</code>.</p><p style='color:#555'>4 files changed · +312 −48</p>", "#24292f", "You are receiving this because your review was requested."),
     [], {"seen"}, None),
    ("w-roadmap", WORK, ("Maya Patel", "maya@northwind.dev"), "Q4 roadmap review — slides attached", 12, 15,
     "Hi Lin,\n\nAttached are the slides for Thursday's Q4 roadmap review. Could you look at the sync reliability section (p. 6–9) and add numbers from the IDLE investigation?\n\nThanks,\nMaya",
     None, [("Q4-roadmap-review.pdf", PDF, pdf(["Northwind - Q4 Roadmap Review", "", "1. Sync reliability", "2. Push notifications", "3. Search v2", "4. Android beta"]))], {"seen", "flagged"}, None),
    ("w-roadmap-re", WORK, ("Jonas Weber", "jonas@northwind.dev"), "Re: Q4 roadmap review — slides attached", 12, 17,
     "Looks good. One note: the push numbers on slide 11 are from staging, not production. I'll send the prod dashboard tomorrow.\n\n— Jonas\n\n> Attached are the slides for Thursday's Q4 roadmap review.",
     None, [], {"seen"}, "w-roadmap"),
    ("q-jd", CN, ("京东", "jd@jd.com"), "【京东】您的订单已发货，预计11月3日送达", 11, 9,
     "您好，您的订单 2861 7730 4415 已由京东物流发出，预计 11月3日 送达。",
     html_card("您的订单已发货", "<p>尊敬的用户，您好：</p><p>您购买的 <b>Keychron K3 Pro 矮轴机械键盘（青轴）</b> 已由京东物流发出。</p><p>订单号：2861 7730 4415<br>预计送达：11月3日（周二）</p><p><a href='#'>查看物流详情</a></p>", "#e1251b", "此邮件由系统自动发送，请勿直接回复。"),
     [], {"seen"}, None),
    ("h-rust", HOME, ("This Week in Rust", "newsletter@this-week-in-rust.org"), "This Week in Rust 571", 11, 7,
     "Hello and welcome to another issue of This Week in Rust!",
     html_card("This Week in Rust 571", "<p>Hello and welcome to another issue of <i>This Week in Rust</i>!</p><h3>Updates from Rust Community</h3><ul><li>Announcing Rust 1.91.0</li><li>Async closures in practice</li><li>Writing an IMAP client without tears</li></ul><h3>Crate of the Week</h3><p>This week's crate is <b>mail-parser</b>, a fast RFC 5322 parser.</p>", "#a72145", "You are receiving this because you subscribed to This Week in Rust."),
     [], set(), None),
    ("w-li", WORK, ("李明", "li.ming@northwind.dev"), "周五上线检查清单", 10, 11,
     "陈琳，\n\n周五晚上 v0.4 上线，检查清单如下：\n1. 数据库迁移脚本在预发环境跑一遍\n2. 推送证书有效期确认（到 2027 年 3 月）\n3. 回滚方案写进文档\n4. 值班表：我 22:00–02:00，王芳 02:00–06:00\n\n有问题随时说。\n李明",
     None, [], {"seen"}, None),
    ("w-wang", WORK, ("王芳", "wang.fang@northwind.dev"), "Re: 周五上线检查清单", 10, 14,
     "第 2 条我确认过了，APNs 和 FCM 的凭据都没问题。\n回滚脚本我今晚补上。\n\n> 周五晚上 v0.4 上线，检查清单如下：",
     None, [], {"seen"}, "w-li"),
    ("h-bank", HOME, ("Harbor Bank", "statements@harborbank.example"), "Your October statement is ready", 9, 8,
     "Your October 2026 statement for account ending 4417 is ready. Closing balance: $8,214.36.",
     html_card("Your October statement is ready", "<p>Hi Lin,</p><p>Your statement for the account ending in <b>•••• 4417</b> is ready.</p><table cellpadding='6' style='border-collapse:collapse;font-size:14px'><tr><td>Opening balance</td><td align='right'>$7,902.10</td></tr><tr><td>Deposits</td><td align='right'>+$6,450.00</td></tr><tr><td>Withdrawals</td><td align='right'>−$6,137.74</td></tr><tr style='border-top:1px solid #ddd;font-weight:600'><td>Closing balance</td><td align='right'>$8,214.36</td></tr></table><p>The PDF statement is attached.</p>", "#0f5132", "Harbor Bank, N.A. Member FDIC."),
     [("Statement-2026-10.pdf", PDF, pdf(["Harbor Bank - Statement October 2026", "Account ending 4417", "Closing balance: $8,214.36"]))], {"seen"}, None),
    ("q-zhang", CN, ("张伟", "zhang.wei@mail.example"), "大学同学聚会通知 · 11月23日", 9, 20,
     "各位老同学：\n\n毕业十周年，定在 11 月 23 日（周日）中午 12 点，地点还是学校东门的“老地方”饭店。\n人均 200，到时候 AA。请回复是否参加，方便订包间。\n\n张伟",
     None, [], {"seen"}, None),
    ("w-inv", WORK, ("Hetzner Online", "billing@hetzner.com"), "Invoice INV-2026-1107 for your cloud servers", 8, 6,
     "Dear customer,\n\nplease find attached invoice INV-2026-1107 for October 2026. Amount due: €48.73. It will be charged to your card on file on 15 Nov.\n\nKind regards,\nHetzner Online GmbH",
     None, [("INV-2026-1107.pdf", PDF, pdf(["Hetzner Online GmbH", "Invoice INV-2026-1107", "Period: October 2026", "CX32 x2, Volume 200 GB", "Total: EUR 48.73"]))], set(), None),
    ("q-liu", CN, ("刘洋", "liu.yang@mail.example"), "Re: 大学同学聚会通知 · 11月23日", 8, 21,
     "我参加！+1 家属。\n顺便问下有人从上海过去吗，可以拼车。\n\n> 毕业十周年，定在 11 月 23 日（周日）中午 12 点",
     None, [], set(), "q-zhang"),
    ("h-flight", HOME, ("Pacific Airways", "booking@pacificairways.com"), "Booking confirmed: Shanghai (PVG) → Tokyo (NRT), 12 Dec", 1, 13,
     "Your booking is confirmed. Booking reference: K7Q2XM. PA 921, 12 Dec 2026, PVG 09:05 → NRT 12:55.",
     html_card("You're going to Tokyo ✈", "<p>Booking reference <b style='font-size:18px;letter-spacing:2px'>K7Q2XM</b></p><table cellpadding='6' style='font-size:14px'><tr><td><b>PA 921</b></td><td>Sat 12 Dec 2026</td></tr><tr><td>Shanghai Pudong (PVG)</td><td>09:05</td></tr><tr><td>Tokyo Narita (NRT)</td><td>12:55</td></tr><tr><td>Passenger</td><td>CHEN / LIN</td></tr><tr><td>Seat</td><td>23A · Economy</td></tr></table><p>Your e-ticket receipt is attached. Online check-in opens 48 hours before departure.</p>", "#005f73", "Pacific Airways · Manage booking"),
     [("E-ticket-K7Q2XM.pdf", PDF, pdf(["Pacific Airways - Electronic Ticket", "Booking ref K7Q2XM", "PA 921 PVG-NRT 12 DEC 2026 0905", "Passenger: CHEN/LIN  Seat 23A"]))], {"seen", "flagged"}, None),
    ("w-linear", WORK, ("Linear", "notifications@linear.app"), "ENG-482 assigned to you: Search ignores diacritics", 7, 16,
     "Jonas Weber assigned ENG-482 to you.\n\nSearch ignores diacritics: searching \"cafe\" does not find \"café\".\nPriority: Medium · Cycle 31",
     None, [], set(), None),
    ("h-mom", HOME, ("妈妈", "mei@chen-family.example"), "周末回家吃饭吗？", 6, 19,
     "琳琳：\n\n这周六你爸生日，回家吃饭吧，我做红烧肉。\n附上上次去西湖拍的照片，你看看哪张洗出来好。\n\n妈妈",
     None, [("西湖.png", PNG, png(320, 200, (120, 170, 220), (40, 90, 60)))], {"seen"}, None),
    ("h-dad", HOME, ("爸爸", "jian@chen-family.example"), "Re: 周末回家吃饭吗？", 6, 21,
     "不用买礼物，人回来就行。路上注意安全。\n\n> 这周六你爸生日，回家吃饭吧",
     None, [], set(), "h-mom"),
    ("q-alipay", CN, ("支付宝", "service@mail.alipay.com"), "您的 10 月账单已出", 5, 10,
     "亲爱的用户，您 2026 年 10 月的账单已出：支出 ¥6,382.40，收入 ¥15,200.00。明细见附件。",
     html_card("10 月账单", "<p>亲爱的用户：</p><p>您 2026 年 10 月共支出 <b>¥6,382.40</b>，收入 <b>¥15,200.00</b>。</p><p>支出最多的分类：餐饮美食（¥1,904.20）、交通出行（¥1,120.00）。</p><p>账单明细见附件 Excel。</p>", "#1677ff", "本邮件由支付宝系统发出。"),
     [("2026年10月账单明细.xlsx", XLSX, xlsx([["日期", "分类", "说明", "金额"], ["2026-10-02", "餐饮美食", "瑞幸咖啡", -18.0], ["2026-10-05", "交通出行", "滴滴出行", -46.5], ["2026-10-10", "工资", "Northwind", 15200.0]]))], set(), None),
    ("w-invite", WORK, ("Maya Patel", "maya@northwind.dev"), "Invitation: Design sync @ Thu Nov 14, 10:00 – 10:45 (CST)", 4, 9,
     "Maya Patel has invited you to: Design sync\nThu Nov 14, 2026 10:00 – 10:45 (CST)\nRoom 4B / Meet\n\nAgenda: reader layout, attachment chips, dark mode.",
     html_card("Design sync", "<p><b>Thu Nov 14, 2026 · 10:00 – 10:45 (CST)</b><br>Room 4B / Google Meet</p><p>Agenda:</p><ol><li>Reader layout on wide screens</li><li>Attachment chips</li><li>Dark mode contrast</li></ol><p>Going? <a href='#'>Yes</a> · <a href='#'>Maybe</a> · <a href='#'>No</a></p>", "#5b3cc4", "Invitation from Northwind Calendar"),
     [("invite.ics", "text/calendar", ICS.encode())], set(), None),
    ("q-12306", CN, ("中国铁路12306", "12306@rails.com.cn"), "网上购票系统-用户支付通知", 4, 15,
     "尊敬的 陈女士：\n\n您好！您于 2026 年 11 月 9 日在中国铁路客户服务中心网站成功购买了 1 张车票，票款共计 553.00 元，详细信息如下：\n1. 陈女士，11月15日 G7 次，上海虹桥站 09:00 开，北京南站 13:28 到，二等座 05 车 12F 号。\n\n请凭购票时所使用的有效身份证件原件进站乘车。",
     None, [], {"flagged"}, None),
    ("h-pragmatic", HOME, ("The Pragmatic Engineer", "gergely@pragmaticengineer.com"), "The state of AI coding tools in late 2026", 3, 7,
     "In today's issue: what engineers actually use, what they pay for, and what they abandoned.",
     html_card("The Pragmatic Engineer", "<h2 style='margin-top:0'>The state of AI coding tools in late 2026</h2><p>We surveyed 3,000+ engineers about what they actually use day to day, what they pay for, and what they tried and abandoned.</p><p><b>In this issue:</b></p><ul><li>Agents in the terminal vs. in the IDE</li><li>Who pays: individuals or employers?</li><li>Code review is the new bottleneck</li></ul><p><a href='#'>Read the full article →</a></p>", "#111", "You're a free subscriber. Upgrade for full access."),
     [], set(), None),
    ("w-expense", WORK, ("Expensify", "concierge@expensify.com"), "Your expense report \"Tokyo offsite\" was approved", 3, 12,
     "Your report \"Tokyo offsite\" ($1,284.50, 9 expenses) was approved by Maya Patel and will be reimbursed in the next payroll.",
     None, [], {"seen"}, None),
    ("q-douban", CN, ("豆瓣读书", "noreply@douban.com"), "豆瓣读书周报：本周新书速递", 3, 18,
     "本周新书速递：《置身事内》新版、《长安的荔枝》插图本……",
     html_card("本周新书速递", "<ul><li><b>《置身事内》（增订版）</b> 兰小欢 · 评分 9.1</li><li><b>《长安的荔枝》插图本</b> 马伯庸 · 评分 8.6</li><li><b>《Rust 程序设计（第 2 版）》</b> · 评分 9.3</li></ul><p><a href='#'>去豆瓣看看 →</a></p>", "#007722", "退订请点击这里。"),
     [], set(), None),
    ("h-tom", HOME, ("Tom Hughes", "tom.hughes@mail.example"), "Photos from the hike", 2, 20,
     "Hey Lin,\n\nGreat hike on Saturday! Here's the summit shot. Next time let's start earlier — the fog rolled in right after we got down.\n\nTom",
     None, [("summit.png", PNG, png(400, 260, (250, 200, 120), (60, 80, 140)))], set(), None),
    ("w-gh-issue", WORK, ("Jonas Weber", "notifications@github.com"), "Re: [northwind/atlas] IDLE stalls after first message (Issue #10)", 2, 10,
     "Confirmed fixed by #11 on my side — left the client running overnight and it picked up all 37 messages.\n\nReply to this email directly or view it on GitHub.",
     None, [], {"seen"}, None),
    ("w-hr", WORK, ("Northwind People Team", "people@northwind.dev"), "Reminder: benefits enrollment closes Nov 20", 1, 9,
     "Open enrollment for 2027 benefits closes on Friday, Nov 20. If you do nothing, your current elections roll over.",
     html_card("Benefits enrollment closes Nov 20", "<p>Hi Lin,</p><p>Open enrollment for 2027 benefits closes on <b>Friday, November 20</b>. If you do nothing, your current elections roll over — except the FSA, which must be re-elected every year.</p><p><a href='#'>Review my benefits</a></p>", "#c2410c", "Northwind People Team"),
     [], set(), None),
    ("h-airbnb", HOME, ("Airbnb", "automated@airbnb.com"), "Reminder: your trip to Kyoto is coming up", 1, 14,
     "Your stay at \"Machiya near Gion\" is Dec 14 – 18. Check-in after 15:00. Your host Yuki will send directions a few days before.",
     None, [], set(), None),
    ("w-sarah-lunch", WORK, ("Maya Patel", "maya@northwind.dev"), "Lunch tomorrow?", 0, 8,
     "Ramen place on 5th at 12:15? I want to talk about the dark mode stuff before the design sync.",
     None, [], {"seen"}, None),
]



def build(entry):
    key, to, (name, addr), subject, days, hour, text, html, atts, flags, reply = entry
    m = EmailMessage()
    m["From"] = f"{name} <{addr}>" if name.isascii() else f"=?utf-8?b?{__import__('base64').b64encode(name.encode()).decode()}?= <{addr}>"
    m["To"] = to
    m["Subject"] = subject
    when = (NOW - timedelta(days=days)).replace(hour=hour, minute=zlib.crc32(key.encode()) % 50 + 3)
    m["Date"] = format_datetime(when)
    mid = f"<{key}.2026@{addr.split('@')[1]}>"
    m["Message-ID"] = mid
    if reply:
        rid = next(f"<{x[0]}.2026@{x[2][1].split('@')[1]}>" for x in M if x[0] == reply)
        m["In-Reply-To"] = rid
        m["References"] = rid
    m.set_content(text)
    if html:
        m.add_alternative(html, subtype="html")
    for fname, ctype, data in atts:
        main, sub = ctype.split("/")
        m.add_attachment(data, maintype=main, subtype=sub, filename=fname)
    return m, addr, to, flags


def send(entries):
    out = []
    with smtplib.SMTP("127.0.0.1", SMTP) as s:
        for e in entries:
            m, frm, to, flags = build(e)
            s.sendmail(frm, [to], m.as_bytes())
            out.append((to, m["Message-ID"], flags))
    return out


def mark(sent):
    by_acct = {}
    for to, mid, flags in sent:
        if flags:
            by_acct.setdefault(to, []).append((mid, flags))
    for acct, items in by_acct.items():
        c = imaplib.IMAP4("127.0.0.1", IMAP)
        c.login(acct, "pw")
        c.select("INBOX")
        _, data = c.uid("FETCH", "1:*", "(BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
        uid_of = {}
        for part in data:
            if isinstance(part, tuple):
                uid = part[0].decode().split("UID ")[1].split()[0]
                hdr = part[1].decode().strip().split(":", 1)[1].strip()
                uid_of[hdr] = uid
        for mid, flags in items:
            fl = " ".join({"seen": "\\Seen", "flagged": "\\Flagged"}[f] for f in flags)
            c.uid("STORE", uid_of[mid], "+FLAGS", f"({fl})")
        c.logout()


if __name__ == "__main__":
    sent = send(M)
    mark(sent)
    print(len(sent), "messages seeded")
