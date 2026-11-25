// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/painting.dart';

/// WCAG AA threshold for body-size text and labels on a fill.
const minTextContrast = 4.5;

/// WCAG contrast ratio between two colours, 1.0 (none) to 21.0 (black on white).
double contrastRatio(Color a, Color b) {
  final la = a.computeLuminance();
  final lb = b.computeLuminance();
  final hi = la > lb ? la : lb;
  final lo = la > lb ? lb : la;
  return (hi + 0.05) / (lo + 0.05);
}

/// True when white label text on [fill] reads at AA. Brand colours must pass this.
bool whiteTextReadsOn(Color fill) =>
    contrastRatio(const Color(0xFFFFFFFF), fill) >= minTextContrast;

/// Dark label ink for light fills (the dark canvas colour).
const fillInk = Color(0xFF0B1217);

/// The label colour for text on an owner-chosen [fill]: white when it reads at AA, else [fillInk], else
/// black (a narrow mid-grey band where #0B1217 falls just short and only pure black reaches 4.5:1).
Color labelOnFill(Color fill) {
  if (whiteTextReadsOn(fill)) return const Color(0xFFFFFFFF);
  if (contrastRatio(fillInk, fill) >= minTextContrast) return fillInk;
  return const Color(0xFF000000);
}

/// [colour], darkened in 1% lightness steps until it reads at AA on every one of [backgrounds]. An owner's
/// primary that already passes comes back unchanged.
Color textSafeOn(Color colour, List<Color> backgrounds) {
  var hsl = HSLColor.fromColor(colour);
  bool reads(Color c) =>
      backgrounds.every((bg) => contrastRatio(c, bg) >= minTextContrast);
  while (!reads(hsl.toColor()) && hsl.lightness > 0) {
    hsl = hsl.withLightness((hsl.lightness - 0.01).clamp(0.0, 1.0));
  }
  return hsl.toColor();
}
