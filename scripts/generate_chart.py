#!/usr/bin/env python3
"""Generate a single comprehensive chart: commits/day bars + conformance/emit/fourslash lines."""

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import matplotlib.ticker as mticker
from datetime import datetime, timedelta
import numpy as np

# ── Theme ─────────────────────────────────────────────────────────────────
BG = '#0D1117'
PANEL = '#161B22'
BORDER = '#30363D'
TEXT = '#C9D1D9'
TEXT_DIM = '#8B949E'
GREEN = '#3FB950'
ORANGE = '#F5A623'
CYAN = '#39D2C0'
PINK = '#F778BA'
BAR_COLOR = '#238636'
BAR_EDGE = '#2EA043'

# ── Data ──────────────────────────────────────────────────────────────────

# Commits per day (full history from git log)
commits_raw = {
    '2026-01-17': 215, '2026-01-18': 87, '2026-01-19': 84, '2026-01-20': 40,
    '2026-01-21': 93, '2026-01-22': 128, '2026-01-23': 216, '2026-01-24': 527,
    '2026-01-25': 96, '2026-01-26': 80, '2026-01-27': 86, '2026-01-28': 75,
    '2026-01-29': 140, '2026-01-30': 94, '2026-01-31': 217,
    '2026-02-01': 77, '2026-02-02': 178, '2026-02-03': 161, '2026-02-04': 692,
    '2026-02-05': 774, '2026-02-06': 457, '2026-02-07': 179, '2026-02-08': 131,
    '2026-02-09': 118, '2026-02-10': 179, '2026-02-11': 142, '2026-02-12': 411,
    '2026-02-13': 325, '2026-02-14': 755, '2026-02-15': 491, '2026-02-16': 146,
    '2026-02-17': 211, '2026-02-18': 111, '2026-02-19': 287, '2026-02-20': 208,
    '2026-02-21': 1212, '2026-02-22': 417, '2026-02-23': 295, '2026-02-24': 269,
    '2026-02-25': 261, '2026-02-26': 215, '2026-02-27': 298, '2026-02-28': 275,
    '2026-03-01': 208, '2026-03-02': 149, '2026-03-03': 146, '2026-03-04': 84,
    '2026-03-05': 71, '2026-03-06': 218, '2026-03-07': 240, '2026-03-08': 196,
    '2026-03-09': 233, '2026-03-10': 351, '2026-03-11': 222, '2026-03-12': 159,
    '2026-03-13': 159, '2026-03-14': 180, '2026-03-15': 265,
}

# Progress lines (pass rate %) from README history, using best daily value
# Some early data points are noisy (test suite was being restructured), cleaned up
progress = {
    # date: (conformance%, js_emit%, dts_emit%, fourslash%)
    # None means no data yet for that metric
    '2026-01-22': (36.4, None, None, None),
    '2026-01-23': (37.0, None, None, None),
    '2026-01-29': (39.2, None, None, None),
    '2026-01-30': (46.7, None, None, 4.1),  # use end-of-day settled value
    '2026-01-31': (46.7, None, None, 11.1),
    '2026-02-01': (48.4, 12.8, None, 11.1),
    '2026-02-02': (50.2, 12.8, None, 12.2),
    '2026-02-03': (50.2, 12.8, None, 11.4),
    '2026-02-04': (50.2, 12.8, None, 11.4),  # test infra restructuring
    '2026-02-05': (40.6, 19.4, None, 11.4),
    '2026-02-06': (47.6, 24.5, 22.7, 11.4),
    '2026-02-07': (51.7, 39.5, 22.7, 12.1),
    '2026-02-10': (58.5, 39.5, 16.4, 12.1),
    '2026-02-12': (60.4, 41.3, 15.8, 12.1),
    '2026-02-13': (62.4, 46.0, 15.8, 12.1),
    '2026-02-14': (64.2, 47.0, 15.8, 12.1),
    '2026-02-15': (68.5, 50.8, 39.1, 12.1),
    '2026-02-16': (66.0, 50.8, 31.5, 12.1),
    '2026-02-17': (70.1, 59.3, 31.4, 12.1),
    '2026-02-18': (71.0, 61.7, 31.5, 12.1),
    '2026-02-19': (71.4, 63.4, 31.9, 12.1),
    '2026-02-20': (71.9, 65.2, 31.9, 12.9),
    '2026-02-21': (73.2, 67.7, 33.9, 16.7),
    '2026-02-22': (73.6, 70.0, 39.0, 18.2),  # use conformance from snapshot commits
    '2026-02-23': (63.3, 72.5, 39.0, 18.3),
    '2026-02-24': (63.9, 73.6, 38.9, 18.5),
    '2026-02-25': (68.7, 74.5, 39.0, 18.5),
    '2026-02-26': (73.6, 74.9, 49.2, 18.9),
    '2026-02-27': (74.0, 74.9, 49.2, 18.9),
    '2026-02-28': (77.0, 75.5, 63.9, 18.9),
    '2026-03-01': (77.2, 75.5, 65.5, 38.4),
    '2026-03-02': (77.2, 75.5, 65.5, 38.5),  # fourslash restructured
    '2026-03-03': (78.6, 76.6, 53.3, 99.7),  # fourslash narrowed to 2540 tests
    '2026-03-06': (79.5, 80.8, 59.5, 99.8),
    '2026-03-07': (80.5, 81.3, 58.9, 99.6),
    '2026-03-08': (81.5, 82.4, 64.1, 99.7),
    '2026-03-09': (82.0, 83.8, 68.2, 99.7),
    '2026-03-10': (82.9, 83.9, 70.2, 100.0),
    '2026-03-11': (83.7, 83.9, 70.1, 99.8),
    '2026-03-12': (84.2, 83.9, 70.1, 99.8),
    '2026-03-13': (85.0, 83.9, 69.7, 99.7),
    '2026-03-14': (85.8, 83.9, 69.9, 99.6),
    '2026-03-15': (85.9, 84.6, 70.6, 99.2),  # fourslash re-scoped to 1400 tests
}

# ── Parse dates ───────────────────────────────────────────────────────────

commit_dates = sorted(commits_raw.keys())
all_dates = []
all_counts = []
d = datetime.strptime(commit_dates[0], '%Y-%m-%d')
end = datetime.strptime(commit_dates[-1], '%Y-%m-%d')
while d <= end:
    ds = d.strftime('%Y-%m-%d')
    all_dates.append(d)
    all_counts.append(commits_raw.get(ds, 0))
    d += timedelta(days=1)

# Progress lines
prog_dates_sorted = sorted(progress.keys())
p_dates = [datetime.strptime(d, '%Y-%m-%d') for d in prog_dates_sorted]
p_conf = [progress[d][0] for d in prog_dates_sorted]
p_js = [progress[d][1] for d in prog_dates_sorted]
p_dts = [progress[d][2] for d in prog_dates_sorted]
p_fs = [progress[d][3] for d in prog_dates_sorted]

# Filter out None for each line
def filter_nones(dates, values):
    fd, fv = [], []
    for d, v in zip(dates, values):
        if v is not None:
            fd.append(d)
            fv.append(v)
    return fd, fv

conf_d, conf_v = filter_nones(p_dates, p_conf)
js_d, js_v = filter_nones(p_dates, p_js)
dts_d, dts_v = filter_nones(p_dates, p_dts)
fs_d, fs_v = filter_nones(p_dates, p_fs)

# ── Cumulative commits ────────────────────────────────────────────────────
cum_commits = np.cumsum(all_counts)

# ── Figure ────────────────────────────────────────────────────────────────

fig, ax = plt.subplots(figsize=(22, 10), facecolor=BG)
ax.set_facecolor(PANEL)

# Title
fig.text(0.5, 0.96, 'tsz — TypeScript Compiler in Rust',
         ha='center', fontsize=26, fontweight='bold', color='white', family='monospace')
fig.text(0.5, 0.93, f'Jan 17 – Mar 15, 2026  •  {sum(all_counts):,} commits  •  Conformance / Emit / Fourslash progress',
         ha='center', fontsize=13, color=TEXT_DIM)

# ── Bars: commits/day ─────────────────────────────────────────────────────

bar_width = 0.8
bars = ax.bar(all_dates, all_counts, width=bar_width, color=BAR_COLOR,
              edgecolor=BAR_EDGE, linewidth=0.3, alpha=0.6, zorder=2,
              label=f'Commits/day (total: {sum(all_counts):,})')

# Highlight peak days
for d, c, bar in zip(all_dates, all_counts, bars):
    if c >= 700:
        bar.set_alpha(0.85)
        bar.set_edgecolor('#F5A623')
        bar.set_linewidth(1.0)
        ax.text(d, c + 20, f'{c}', ha='center', va='bottom', fontsize=8,
                color=ORANGE, fontweight='bold')

ax.set_ylabel('Commits per Day', color=TEXT_DIM, fontsize=12, labelpad=10)
ax.set_ylim(0, max(all_counts) * 1.15)

# ── Right axis: progress % ───────────────────────────────────────────────

ax2 = ax.twinx()

# Conformance line
ax2.plot(conf_d, conf_v, color=GREEN, linewidth=3, marker='o', markersize=4,
         zorder=5, label='Conformance')
# JS emit line
ax2.plot(js_d, js_v, color=ORANGE, linewidth=2.5, marker='s', markersize=3.5,
         zorder=5, label='JS Emit')
# DTS emit line
ax2.plot(dts_d, dts_v, color=CYAN, linewidth=2.5, marker='^', markersize=3.5,
         zorder=5, label='DTS Emit')
# Fourslash line
ax2.plot(fs_d, fs_v, color=PINK, linewidth=2, marker='D', markersize=3,
         zorder=5, label='Fourslash', alpha=0.8)

ax2.set_ylabel('Pass Rate (%)', color=TEXT_DIM, fontsize=12, labelpad=10)
ax2.set_ylim(0, 105)
ax2.yaxis.set_major_formatter(mticker.PercentFormatter())

# ── Annotations for key values ────────────────────────────────────────────

# Final values
final_date = datetime(2026, 3, 15)
annotations = [
    (final_date, 85.9, f'85.9%\n10,809/12,581', GREEN, (30, 10)),
    (final_date, 84.6, f'84.6%\n11,446/13,526', ORANGE, (30, -15)),
    (final_date, 70.6, f'70.6%\n1,171/1,659', CYAN, (30, 0)),
    (final_date, 99.2, f'99.2%\n1,389/1,400', PINK, (30, 8)),
]
for d, y, text, color, offset in annotations:
    ax2.annotate(text, xy=(d, y), xytext=offset, textcoords='offset points',
                fontsize=9, color=color, fontweight='bold',
                arrowprops=dict(arrowstyle='->', color=color, lw=1.2))

# Starting conformance value
ax2.annotate('36.4%', xy=(conf_d[0], conf_v[0]), xytext=(-5, 12),
             textcoords='offset points', fontsize=9, color=GREEN, fontweight='bold')

# Starting JS emit
ax2.annotate('12.8%', xy=(js_d[0], js_v[0]), xytext=(-5, -15),
             textcoords='offset points', fontsize=9, color=ORANGE, fontweight='bold')

# Key milestone annotations
# Fourslash jump
ax2.annotate('fourslash\nrescoped\n→ 99.7%',
             xy=(datetime(2026, 3, 3), 99.7),
             xytext=(-60, -35), textcoords='offset points',
             fontsize=8, color=PINK, fontstyle='italic',
             arrowprops=dict(arrowstyle='->', color=PINK, lw=1))

# DTS jump
ax2.annotate('DTS\n+32pp',
             xy=(datetime(2026, 2, 28), 63.9),
             xytext=(-40, -25), textcoords='offset points',
             fontsize=8, color=CYAN, fontstyle='italic',
             arrowprops=dict(arrowstyle='->', color=CYAN, lw=1))

# Peak commit day
ax2.annotate('1,212 commits\n(peak day)',
             xy=(datetime(2026, 2, 21), 5),
             xytext=(5, 20), textcoords='offset points',
             fontsize=8, color=ORANGE, fontstyle='italic',
             arrowprops=dict(arrowstyle='->', color=ORANGE, lw=1))

# ── Style ─────────────────────────────────────────────────────────────────

ax.tick_params(colors=TEXT_DIM, labelsize=9)
ax2.tick_params(colors=TEXT_DIM, labelsize=9)
for spine in ax.spines.values():
    spine.set_color(BORDER)
for spine in ax2.spines.values():
    spine.set_color(BORDER)

ax.xaxis.set_major_locator(mdates.WeekdayLocator(byweekday=0))  # Mondays
ax.xaxis.set_major_formatter(mdates.DateFormatter('%b %d'))
ax.xaxis.set_minor_locator(mdates.DayLocator())
ax.tick_params(axis='x', which='minor', length=0)
plt.setp(ax.xaxis.get_majorticklabels(), rotation=0, ha='center')

ax.set_xlim(all_dates[0] - timedelta(days=1), all_dates[-1] + timedelta(days=1.5))
ax.grid(axis='y', color=BORDER, alpha=0.3, linewidth=0.5)
ax2.grid(False)

# ── Legends ───────────────────────────────────────────────────────────────

# Combine legends from both axes
lines1, labels1 = ax.get_legend_handles_labels()
lines2, labels2 = ax2.get_legend_handles_labels()
legend = ax.legend(lines1 + lines2, labels1 + labels2,
                   loc='upper left', fontsize=11, facecolor=PANEL, edgecolor=BORDER,
                   labelcolor=TEXT, framealpha=0.95)
legend.set_zorder(10)

# ── Week separators ───────────────────────────────────────────────────────
d = datetime(2026, 1, 20)  # first Monday
while d <= end:
    ax.axvline(x=d, color=BORDER, linewidth=0.5, alpha=0.4, linestyle='-', zorder=1)
    d += timedelta(weeks=1)

# ── Save ──────────────────────────────────────────────────────────────────

plt.subplots_adjust(left=0.06, right=0.91, top=0.89, bottom=0.08)
plt.savefig('/home/user/tsz/git_activity.png', dpi=150, facecolor=fig.get_facecolor(),
            bbox_inches='tight', pad_inches=0.4)
plt.close()
print("Chart saved to git_activity.png")
