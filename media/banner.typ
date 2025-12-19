#let background = color.hsl(75deg, 80%, 25%)
#let foreground = background.lighten(80%)

#set page(
  width: 1073pt,
  height: 151pt,
  margin: 0em,
  fill: none,
  background: {
    box(
      width: 100%,
      height: 100%,
      fill: background,
      radius: 10%,
      block(
        width: 95%,
        height: 70%,
        fill: tiling(
          size: (2cm, .8cm),
          text(
            size: 20pt,
            font: "Libertinus Sans",
            fill: background.darken(20%),
          )[Aa],
        ),
      ),
    )
  },
)

#set text(
  size: 70pt,
  fill: foreground,
  spacing: 10pt,
  font: "Libertinus Sans",
)
#set align(center + horizon)

#text(1.9em)[*font*]
ain
