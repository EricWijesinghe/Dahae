// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/material.dart';

// The shared theme gives FilledButton a full-width minimum (the customer app's bottom CTAs). Beside other
// content (a ListTile slot, a Row, a Wrap, dialog actions) that squeezes the neighbours to one letter per
// line or cannot lay out at all, so console buttons placed inline take one of these instead.

/// Rows and Wraps: as tall as the theme's OutlinedButton, so mixed button rows line up.
final inlineFilled = FilledButton.styleFrom(minimumSize: const Size(64, 52));

/// ListTile trailing slots and dialog actions: as tall as the theme's TextButton.
final compactFilled = FilledButton.styleFrom(minimumSize: const Size(64, 44));
