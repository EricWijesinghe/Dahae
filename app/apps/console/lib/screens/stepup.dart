// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/inline_button.dart';

/// Runs a sensitive owner action. When the server answers 403 stepup_required (no fresh TOTP step-up),
/// asks for an authenticator code, posts it, and runs the action once more. Cancelling rethrows that error.
Future<R> withStepUp<R>(
  BuildContext context,
  Future<R> Function() action,
) async {
  try {
    return await action();
  } on ApiException catch (e) {
    if (e.status != 403 || e.code != 'stepup_required') rethrow;
    if (!context.mounted) rethrow;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => const _StepUpDialog(),
    );
    if (confirmed != true) rethrow;
    if (!context.mounted) rethrow;
    return action();
  }
}

class _StepUpDialog extends StatefulWidget {
  const _StepUpDialog();

  @override
  State<_StepUpDialog> createState() => _StepUpDialogState();
}

class _StepUpDialogState extends State<_StepUpDialog> {
  final _code = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _code.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await context.api.post('/auth/staff/mfa/step-up', {
        'code': _code.text.trim(),
      });
      if (mounted) Navigator.of(context).pop(true);
    } on ApiException catch (e) {
      if (mounted) setState(() => _error = _message(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _message(ApiException e) {
    final t = context.t;
    return switch (e.status) {
      401 => t.cMfaExpired,
      429 => t.cMfaLocked,
      400 when e.code == 'mfa_invalid_code' => t.cMfaInvalid,
      _ => t.error(e),
    };
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return AlertDialog(
      // The page gutter instead of the default 40px inset: at phone width "Confirm and continue" then still fits
      // beside Cancel instead of dropping below it.
      insetPadding: const EdgeInsets.symmetric(
        horizontal: Space.gutter,
        vertical: Space.xl,
      ),
      title: KeepAllText(t.cStepUpTitle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KeepAllText(t.cStepUpBody, style: context.text.bodyMedium),
          const SizedBox(height: Space.md),
          TextField(
            controller: _code,
            keyboardType: TextInputType.number,
            autofocus: true,
            decoration: InputDecoration(labelText: t.cMfaCode),
            onSubmitted: (_) => _submit(),
          ),
          if (_error != null) ...[
            const SizedBox(height: Space.sm),
            KeepAllText(_error!, style: TextStyle(color: context.colors.danger)),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(false),
          child: KeepAllText(t.cStepUpCancel),
        ),
        // PrimaryButton takes no style: the compact size comes from the theme, under its own gradient style.
        Theme(
          data: Theme.of(context).copyWith(
            filledButtonTheme: FilledButtonThemeData(
              style: compactFilled.merge(
                Theme.of(context).filledButtonTheme.style,
              ),
            ),
          ),
          child: PrimaryButton(
            label: t.cStepUpConfirm,
            busy: _busy,
            onPressed: _submit,
          ),
        ),
      ],
    );
  }
}
