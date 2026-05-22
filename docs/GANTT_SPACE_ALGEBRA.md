# Gantt Space Algebra

This note explains the space expression in
[`examples/gantt.ag`](../examples/gantt.ag):

```ag
Space((start_date + end_date) * (attorney / phase)) {
    Rect(
        xmin: start_date,
        xmax: end_date,
        ymin: phase,
        ymax: phase,
        fill: phase,
        alpha: 0.85,
    )
}
```

The important rule is that Algraf's algebra operators are not arithmetic.

`start_date + end_date` does not add dates.

`attorney / phase` does not divide strings.

`*` does not multiply anything.

Inside `Space(...)`, these operators describe the structure of the visual
coordinate system.

## The Expression

The expression:

```ag
(start_date + end_date) * (attorney / phase)
```

means:

```text
Cartesian(
  Union(start_date, end_date),
  Nested(attorney, phase)
)
```

In chart terms:

```text
x axis = one temporal timeline trained from both start_date and end_date
y axis = attorney bands, with phase sub-bands inside each attorney
```

That is the whole Gantt layout.

## `+`: Blend Date Domains

The left side is:

```ag
start_date + end_date
```

Algraf calls `+` the blend operator. It creates a shared domain from multiple
columns. For this Gantt chart, both columns are temporal, so the x scale is a
single timeline that covers the earliest start date through the latest end
date.

Using the sample data in [`examples/gantt.csv`](../examples/gantt.csv), the x
domain needs to include:

```text
earliest start_date = 2026-06-01
latest end_date     = 2026-07-20
```

The result is not a calculated date value. It is one x scale whose domain is
large enough for both ends of every interval.

This matters because the `Rect` mark draws each task from:

```ag
xmin: start_date
xmax: end_date
```

So the scale must understand both the left edge and right edge of the
rectangle.

The parentheses around `(start_date + end_date)` are intentional. Algraf
requires blend expressions to be parenthesized so interval-style scales are
explicit.

## `*`: Cross X and Y

The middle operator is:

```ag
(start_date + end_date) * (attorney / phase)
```

Algraf calls `*` the cross operator. It creates a Cartesian space.

The left operand becomes the horizontal dimension. The right operand becomes
the vertical dimension.

So this part means:

```text
put the blended date timeline on x
put the nested attorney/phase bands on y
```

It is similar to ordinary examples like:

```ag
Space(time * value)
```

except the x side is an interval domain and the y side is a nested categorical
domain.

## `/`: Nest Phase Inside Attorney

The right side is:

```ag
attorney / phase
```

Algraf calls `/` the nest operator. It allocates an outer band for the left
column, then sub-bands for the right column inside each outer band.

Here that means:

```text
Morgan
  Intake
  Discovery
  Briefing
  Hearing

Reyes
  Intake
  Discovery
  Briefing
  Hearing

Patel
  Intake
  Discovery
  Briefing
  Hearing
```

Only rows that exist in the data draw rectangles, but the nested band scale has
a consistent phase slot inside each attorney band.

For example, this row:

```csv
Morgan,Discovery,2026-06-07,2026-06-20
```

is drawn as:

```text
x left  = 2026-06-07
x right = 2026-06-20
y band  = Morgan's Discovery sub-band
fill    = Discovery color
```

The y guide is labeled `"Attorney / phase"` because both columns participate in
the vertical layout. The visible y ticks are the outer attorney bands; phase is
shown by the sub-band position and by the fill legend.

## How `Rect` Uses The Space

`Space(...)` defines the coordinate system. `Rect(...)` defines one rectangle
per row in that coordinate system.

For each row, `Rect` maps:

```ag
xmin: start_date
xmax: end_date
ymin: phase
ymax: phase
```

The x bounds are temporal values, so they become left and right pixel
positions on the blended timeline.

The y bounds are categorical values. Because the y axis is nested
`attorney / phase`, mapping `phase` as both `ymin` and `ymax` resolves to the
lower and upper edges of that row's phase sub-band inside that row's attorney
band. In other words, `ymin: phase` and `ymax: phase` do not make a zero-height
rectangle. They ask Algraf to fill the categorical phase band.

That gives each task a horizontal bar:

```text
left edge    = start_date
right edge   = end_date
vertical row = attorney + phase slot
color        = phase
```

## Why Not A Simpler Space?

These alternatives describe different charts:

```ag
Space(start_date * attorney)
```

This says the chart's formal x dimension is just `start_date`, and the y
dimension is just `attorney`. It does not say, algebraically, that the mark is
an interval whose x domain depends on both start and end.

```ag
Space((start_date + end_date) * attorney)
```

This gives a correct interval timeline, but all phases for the same attorney
share one vertical attorney band. Phase can still be color, but it no longer
has its own vertical slot.

```ag
Space((start_date + end_date) * phase)
```

This groups by phase only. It loses the attorney grouping.

```ag
Space((start_date + end_date) * (phase / attorney))
```

This reverses the nesting. The outer y bands become phases, and attorneys
become sub-bands inside each phase.

The original expression:

```ag
Space((start_date + end_date) * (attorney / phase))
```

is the one that says:

```text
draw intervals across time,
group them first by attorney,
and split each attorney lane by phase.
```

## Mental Model

Read the Gantt space expression from the outside in:

1. `A * B` means "put `A` on x and `B` on y."
2. `start_date + end_date` means "train one x scale that covers both date columns."
3. `attorney / phase` means "make attorney the outer y category and phase the inner y category."
4. `Rect` then fills the rectangle bounded by each row's start date, end date,
   and nested phase band.

So the expression is not calculating values. It is declaring the shape of the
space that the rectangles will occupy.
