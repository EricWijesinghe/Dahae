// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';

/// Setup step 0: the console's own language. Every shipped language, whatever the country; the choice is this
/// device's, like the language picker everywhere else.
class SetupLanguageStep extends StatelessWidget {
  const SetupLanguageStep({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final current = Localizations.localeOf(context).languageCode;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KeepAllText(t.cLangStepTitle, style: context.text.titleMedium),
        const SizedBox(height: Space.xs),
        KeepAllText(t.cLangStepBody, style: context.text.bodyMedium),
        const SizedBox(height: Space.sm),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.sm,
          children: [
            for (final MapEntry(key: code, value: name)
                in LocaleController.names.entries)
              ChoiceChip(
                label: Text(name),
                selected: code == current,
                onSelected: (_) => Deps.read(context).locale.set(Locale(code)),
              ),
          ],
        ),
      ],
    );
  }
}
