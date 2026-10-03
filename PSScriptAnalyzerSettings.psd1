@{
    Severity     = @('Error', 'Warning')
    ExcludeRules = @(
        'PSAvoidUsingWriteHost',                        # it is a TUI
        'PSAvoidUsingEmptyCatchBlock',                  # locked files / missing keys are expected and skipped on purpose
        'PSUseShouldProcessForStateChangingFunctions',  # dry run is handled by $script:DryRun
        'PSUseSingularNouns',
        'PSUseApprovedVerbs',
        'PSAvoidUsingPositionalParameters',
        'PSUseBOMForUnicodeEncodedFile',
        'PSReviewUnusedParameter'
    )
}
