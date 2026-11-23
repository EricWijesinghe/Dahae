// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart' show KeepAllText;
import '../widgets/inline_button.dart';

/// Asks before a destructive action. [action] names both the title and the confirm button, so the owner
/// sees exactly what they are agreeing to. Resolves false when cancelled or dismissed.
Future<bool> confirmAction(
  BuildContext context,
  String action, {
  String? message,
}) async {
  final t = context.t;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: KeepAllText(action),
      content: KeepAllText(message ?? t.cConfirmAsk),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(ctx).pop(false),
          child: KeepAllText(t.cancel),
        ),
        FilledButton(
          style: compactFilled,
          onPressed: () => Navigator.of(ctx).pop(true),
          child: KeepAllText(action),
        ),
      ],
    ),
  );
  return ok ?? false;
}
