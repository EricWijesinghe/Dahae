// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/widgets.dart';
import 'package:ui/ui.dart';

/// The console title in the UI language. In English the brand's English name
/// leads; a brand with no English name goes after the title, set apart, so the
/// line does not read as one half-Korean phrase.
String consoleTitle(BuildContext context) {
  final title = T.of(context).cTitle;
  final brand = BrandScope.of(context);
  if (context.lang == 'en' && brand.nameEn == null) {
    return '$title · ${brand.name}';
  }
  return '${context.brandName} $title';
}
