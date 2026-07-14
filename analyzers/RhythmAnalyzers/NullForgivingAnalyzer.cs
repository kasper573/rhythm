using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Diagnostics;

namespace RhythmAnalyzers;

/// <summary>
/// Reports every use of the null-forgiving operator <c>!</c>. Nullability must
/// be handled explicitly (<c>?? throw</c>, pattern matching, a throwing
/// accessor); <c>!</c> only silences the NRT warnings that are otherwise errors.
/// </summary>
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class NullForgivingAnalyzer : DiagnosticAnalyzer
{
    public const string DiagnosticId = "RH0001";

    private static readonly DiagnosticDescriptor Rule = new(
        id: DiagnosticId,
        title: "Null-forgiving operator is forbidden",
        messageFormat: "Do not use the null-forgiving operator '!'; handle nullability explicitly",
        category: "Nullability",
        defaultSeverity: DiagnosticSeverity.Warning,
        isEnabledByDefault: true,
        description: "The null-forgiving operator suppresses nullable-reference warnings instead of resolving them.");

    public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics => ImmutableArray.Create(Rule);

    public override void Initialize(AnalysisContext context)
    {
        context.ConfigureGeneratedCodeAnalysis(GeneratedCodeAnalysisFlags.None);
        context.EnableConcurrentExecution();
        context.RegisterSyntaxNodeAction(Report, SyntaxKind.SuppressNullableWarningExpression);
    }

    private static void Report(SyntaxNodeAnalysisContext context)
    {
        var suppression = (Microsoft.CodeAnalysis.CSharp.Syntax.PostfixUnaryExpressionSyntax)context.Node;
        context.ReportDiagnostic(Diagnostic.Create(Rule, suppression.OperatorToken.GetLocation()));
    }
}
