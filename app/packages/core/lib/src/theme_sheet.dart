// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart' show KeepAllText;

/// Bottom sheet with the three theme choices; the current one is checked. Picking one applies and closes.
Future<void> showThemeSheet(BuildContext context, ThemeController theme) {
  return showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (sheetContext) => SafeArea(
      child: ListenableBuilder(
        listenable: theme,
        builder: (context, _) {
          final t = context.t;
          return Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsetsDirectional.fromSTEB(20, 4, 20, 8),
                child: KeepAllText(
                  t.themeTitle,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              for (final mode in const [
                ThemeMode.light,
                ThemeMode.dark,
                ThemeMode.system,
              ])
                ListTile(
                  key: ValueKey('theme_${mode.name}'),
                  title: KeepAllText(t.themeLabel(mode)),
                  trailing: theme.mode == mode ? const Icon(Icons.check) : null,
                  selected: theme.mode == mode,
                  onTap: () async {
                    await theme.set(mode);
                    if (sheetContext.mounted) Navigator.of(sheetContext).pop();
                  },
                ),
            ],
          );
        },
      ),
    ),
  );
}
