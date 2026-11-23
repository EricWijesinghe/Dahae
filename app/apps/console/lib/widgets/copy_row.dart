// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:ui/ui.dart';

/// A label, a selectable value and a copy button.
class CopyRow extends StatelessWidget {
  const CopyRow({super.key, required this.label, required this.value});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: Space.sm),
    child: ListTile(
      contentPadding: EdgeInsets.zero,
      title: KeepAllText(label, style: context.text.labelLarge),
      subtitle: SelectableText(value),
      trailing: IconButton(
        tooltip: context.t.cCopy,
        icon: const Icon(Icons.copy_rounded),
        onPressed: () async {
          await Clipboard.setData(ClipboardData(text: value));
          if (context.mounted) showToast(context, context.t.cCopied);
        },
      ),
    ),
  );
}
