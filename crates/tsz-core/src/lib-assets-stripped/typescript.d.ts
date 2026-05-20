declare namespace ts {
    namespace server {
        namespace protocol {
            export import ApplicableRefactorInfo = ts.ApplicableRefactorInfo;
            export import ClassificationType = ts.ClassificationType;
            export import CompletionsTriggerCharacter = ts.CompletionsTriggerCharacter;
            export import CompletionTriggerKind = ts.CompletionTriggerKind;
            export import InlayHintKind = ts.InlayHintKind;
            export import OrganizeImportsMode = ts.OrganizeImportsMode;
            export import RefactorActionInfo = ts.RefactorActionInfo;
            export import RefactorTriggerReason = ts.RefactorTriggerReason;
            export import RenameInfoFailure = ts.RenameInfoFailure;
            export import SemicolonPreference = ts.SemicolonPreference;
            export import SignatureHelpCharacterTypedReason = ts.SignatureHelpCharacterTypedReason;
            export import SignatureHelpInvokedReason = ts.SignatureHelpInvokedReason;
            export import SignatureHelpParameter = ts.SignatureHelpParameter;
            export import SignatureHelpRetriggerCharacter = ts.SignatureHelpRetriggerCharacter;
            export import SignatureHelpRetriggeredReason = ts.SignatureHelpRetriggeredReason;
            export import SignatureHelpTriggerCharacter = ts.SignatureHelpTriggerCharacter;
            export import SignatureHelpTriggerReason = ts.SignatureHelpTriggerReason;
            export import SymbolDisplayPart = ts.SymbolDisplayPart;
            export import UserPreferences = ts.UserPreferences;
            type ChangePropertyTypes<
                T,
                Substitutions extends {
                    [K in keyof T]?: any;
                },
            > = {
                [K in keyof T]: K extends keyof Substitutions ? Substitutions[K] : T[K];
            };
            type ChangeStringIndexSignature<T, NewStringIndexSignatureType> = {
                [K in keyof T]: string extends K ? NewStringIndexSignatureType : T[K];
            };
            export enum CommandTypes {
                JsxClosingTag = "jsxClosingTag",
                LinkedEditingRange = "linkedEditingRange",
                Brace = "brace",
                BraceCompletion = "braceCompletion",
                GetSpanOfEnclosingComment = "getSpanOfEnclosingComment",
                Change = "change",
                Close = "close",
                Completions = "completions",
                CompletionInfo = "completionInfo",
                CompletionDetails = "completionEntryDetails",
                CompileOnSaveAffectedFileList = "compileOnSaveAffectedFileList",
                CompileOnSaveEmitFile = "compileOnSaveEmitFile",
                Configure = "configure",
                Definition = "definition",
                DefinitionAndBoundSpan = "definitionAndBoundSpan",
                Implementation = "implementation",
                Exit = "exit",
                FileReferences = "fileReferences",
                Format = "format",
                Formatonkey = "formatonkey",
                Geterr = "geterr",
                GeterrForProject = "geterrForProject",
                SemanticDiagnosticsSync = "semanticDiagnosticsSync",
                SyntacticDiagnosticsSync = "syntacticDiagnosticsSync",
                SuggestionDiagnosticsSync = "suggestionDiagnosticsSync",
                NavBar = "navbar",
                Navto = "navto",
                NavTree = "navtree",
                NavTreeFull = "navtree-full",
                DocumentHighlights = "documentHighlights",
                Open = "open",
                Quickinfo = "quickinfo",
                References = "references",
                Reload = "reload",
                Rename = "rename",
                Saveto = "saveto",
                SignatureHelp = "signatureHelp",
                FindSourceDefinition = "findSourceDefinition",
                Status = "status",
                TypeDefinition = "typeDefinition",
                ProjectInfo = "projectInfo",
                ReloadProjects = "reloadProjects",
                Unknown = "unknown",
                OpenExternalProject = "openExternalProject",
                OpenExternalProjects = "openExternalProjects",
                CloseExternalProject = "closeExternalProject",
                UpdateOpen = "updateOpen",
                GetOutliningSpans = "getOutliningSpans",
                TodoComments = "todoComments",
                Indentation = "indentation",
                DocCommentTemplate = "docCommentTemplate",
                CompilerOptionsForInferredProjects = "compilerOptionsForInferredProjects",
                GetCodeFixes = "getCodeFixes",
                GetCombinedCodeFix = "getCombinedCodeFix",
                ApplyCodeActionCommand = "applyCodeActionCommand",
                GetSupportedCodeFixes = "getSupportedCodeFixes",
                GetApplicableRefactors = "getApplicableRefactors",
                GetEditsForRefactor = "getEditsForRefactor",
                GetMoveToRefactoringFileSuggestions = "getMoveToRefactoringFileSuggestions",
                PreparePasteEdits = "preparePasteEdits",
                GetPasteEdits = "getPasteEdits",
                OrganizeImports = "organizeImports",
                GetEditsForFileRename = "getEditsForFileRename",
                ConfigurePlugin = "configurePlugin",
                SelectionRange = "selectionRange",
                ToggleLineComment = "toggleLineComment",
                ToggleMultilineComment = "toggleMultilineComment",
                CommentSelection = "commentSelection",
                UncommentSelection = "uncommentSelection",
                PrepareCallHierarchy = "prepareCallHierarchy",
                ProvideCallHierarchyIncomingCalls = "provideCallHierarchyIncomingCalls",
                ProvideCallHierarchyOutgoingCalls = "provideCallHierarchyOutgoingCalls",
                ProvideInlayHints = "provideInlayHints",
                WatchChange = "watchChange",
                MapCode = "mapCode",
            }
            export interface Message {
                seq: number;
                type: "request" | "response" | "event";
            }
            export interface Request extends Message {
                type: "request";
                command: string;
                arguments?: any;
            }
            export interface ReloadProjectsRequest extends Request {
                command: CommandTypes.ReloadProjects;
            }
            export interface Event extends Message {
                type: "event";
                event: string;
                body?: any;
            }
            export interface Response extends Message {
                type: "response";
                request_seq: number;
                success: boolean;
                command: string;
                message?: string;
                body?: any;
                metadata?: unknown;
                performanceData?: PerformanceData;
            }
            export interface PerformanceData {
                updateGraphDurationMs?: number;
                createAutoImportProviderProgramDurationMs?: number;
                diagnosticsDuration?: FileDiagnosticPerformanceData[];
            }
            export type DiagnosticPerformanceData = {
                [Kind in DiagnosticEventKind]?: number;
            };
            export interface FileDiagnosticPerformanceData extends DiagnosticPerformanceData {
                file: string;
            }
            export interface FileRequestArgs {
                file: string;
                projectFileName?: string;
            }
            export interface StatusRequest extends Request {
                command: CommandTypes.Status;
            }
            export interface StatusResponseBody {
                version: string;
            }
            export interface StatusResponse extends Response {
                body: StatusResponseBody;
            }
            export interface DocCommentTemplateRequest extends FileLocationRequest {
                command: CommandTypes.DocCommentTemplate;
            }
            export interface DocCommandTemplateResponse extends Response {
                body?: TextInsertion;
            }
            export interface TodoCommentRequest extends FileRequest {
                command: CommandTypes.TodoComments;
                arguments: TodoCommentRequestArgs;
            }
            export interface TodoCommentRequestArgs extends FileRequestArgs {
                descriptors: TodoCommentDescriptor[];
            }
            export interface TodoCommentsResponse extends Response {
                body?: TodoComment[];
            }
            export interface SpanOfEnclosingCommentRequest extends FileLocationRequest {
                command: CommandTypes.GetSpanOfEnclosingComment;
                arguments: SpanOfEnclosingCommentRequestArgs;
            }
            export interface SpanOfEnclosingCommentRequestArgs extends FileLocationRequestArgs {
                onlyMultiLine: boolean;
            }
            export interface OutliningSpansRequest extends FileRequest {
                command: CommandTypes.GetOutliningSpans;
            }
            export type OutliningSpan = ChangePropertyTypes<ts.OutliningSpan, {
                textSpan: TextSpan;
                hintSpan: TextSpan;
            }>;
            export interface OutliningSpansResponse extends Response {
                body?: OutliningSpan[];
            }
            export interface IndentationRequest extends FileLocationRequest {
                command: CommandTypes.Indentation;
                arguments: IndentationRequestArgs;
            }
            export interface IndentationResponse extends Response {
                body?: IndentationResult;
            }
            export interface IndentationResult {
                position: number;
                indentation: number;
            }
            export interface IndentationRequestArgs extends FileLocationRequestArgs {
                options?: EditorSettings;
            }
            export interface ProjectInfoRequestArgs extends FileRequestArgs {
                needFileNameList: boolean;
                needDefaultConfiguredProjectInfo?: boolean;
            }
            export interface ProjectInfoRequest extends Request {
                command: CommandTypes.ProjectInfo;
                arguments: ProjectInfoRequestArgs;
            }
            export interface CompilerOptionsDiagnosticsRequest extends Request {
                arguments: CompilerOptionsDiagnosticsRequestArgs;
            }
            export interface CompilerOptionsDiagnosticsRequestArgs {
                projectFileName: string;
            }
            export interface DefaultConfiguredProjectInfo {
                notMatchedByConfig?: readonly string[];
                notInProject?: readonly string[];
                defaultProject?: string;
            }
            export interface ProjectInfo {
                configFileName: string;
                fileNames?: string[];
                languageServiceDisabled?: boolean;
                configuredProjectInfo?: DefaultConfiguredProjectInfo;
            }
            export interface DiagnosticWithLinePosition {
                message: string;
                start: number;
                length: number;
                startLocation: Location;
                endLocation: Location;
                category: string;
                code: number;
                reportsUnnecessary?: {};
                reportsDeprecated?: {};
                relatedInformation?: DiagnosticRelatedInformation[];
            }
            export interface ProjectInfoResponse extends Response {
                body?: ProjectInfo;
            }
            export interface FileRequest extends Request {
                arguments: FileRequestArgs;
            }
            export interface FileLocationRequestArgs extends FileRequestArgs {
                line: number;
                offset: number;
            }
            export type FileLocationOrRangeRequestArgs = FileLocationRequestArgs | FileRangeRequestArgs;
            export interface GetApplicableRefactorsRequest extends Request {
                command: CommandTypes.GetApplicableRefactors;
                arguments: GetApplicableRefactorsRequestArgs;
            }
            export type GetApplicableRefactorsRequestArgs = FileLocationOrRangeRequestArgs & {
                triggerReason?: RefactorTriggerReason;
                kind?: string;
                includeInteractiveActions?: boolean;
            };
            export interface GetApplicableRefactorsResponse extends Response {
                body?: ApplicableRefactorInfo[];
            }
            export interface GetMoveToRefactoringFileSuggestionsRequest extends Request {
                command: CommandTypes.GetMoveToRefactoringFileSuggestions;
                arguments: GetMoveToRefactoringFileSuggestionsRequestArgs;
            }
            export type GetMoveToRefactoringFileSuggestionsRequestArgs = FileLocationOrRangeRequestArgs & {
                kind?: string;
            };
            export interface GetMoveToRefactoringFileSuggestions extends Response {
                body: {
                    newFileName: string;
                    files: string[];
                };
            }
            export interface PreparePasteEditsRequest extends FileRequest {
                command: CommandTypes.PreparePasteEdits;
                arguments: PreparePasteEditsRequestArgs;
            }
            export interface PreparePasteEditsRequestArgs extends FileRequestArgs {
                copiedTextSpan: TextSpan[];
            }
            export interface PreparePasteEditsResponse extends Response {
                body: boolean;
            }
            export interface GetPasteEditsRequest extends Request {
                command: CommandTypes.GetPasteEdits;
                arguments: GetPasteEditsRequestArgs;
            }
            export interface GetPasteEditsRequestArgs extends FileRequestArgs {
                pastedText: string[];
                pasteLocations: TextSpan[];
                copiedFrom?: {
                    file: string;
                    spans: TextSpan[];
                };
            }
            export interface GetPasteEditsResponse extends Response {
                body: PasteEditsAction;
            }
            export interface PasteEditsAction {
                edits: FileCodeEdits[];
                fixId?: {};
            }
            export interface GetEditsForRefactorRequest extends Request {
                command: CommandTypes.GetEditsForRefactor;
                arguments: GetEditsForRefactorRequestArgs;
            }
            export type GetEditsForRefactorRequestArgs = FileLocationOrRangeRequestArgs & {
                refactor: string;
                action: string;
                interactiveRefactorArguments?: InteractiveRefactorArguments;
            };
            export interface GetEditsForRefactorResponse extends Response {
                body?: RefactorEditInfo;
            }
            export interface RefactorEditInfo {
                edits: FileCodeEdits[];
                renameLocation?: Location;
                renameFilename?: string;
                notApplicableReason?: string;
            }
            export interface OrganizeImportsRequest extends Request {
                command: CommandTypes.OrganizeImports;
                arguments: OrganizeImportsRequestArgs;
            }
            export type OrganizeImportsScope = GetCombinedCodeFixScope;
            export interface OrganizeImportsRequestArgs {
                scope: OrganizeImportsScope;
                skipDestructiveCodeActions?: boolean;
                mode?: OrganizeImportsMode;
            }
            export interface OrganizeImportsResponse extends Response {
                body: readonly FileCodeEdits[];
            }
            export interface GetEditsForFileRenameRequest extends Request {
                command: CommandTypes.GetEditsForFileRename;
                arguments: GetEditsForFileRenameRequestArgs;
            }
            export interface GetEditsForFileRenameRequestArgs {
                readonly oldFilePath: string;
                readonly newFilePath: string;
            }
            export interface GetEditsForFileRenameResponse extends Response {
                body: readonly FileCodeEdits[];
            }
            export interface CodeFixRequest extends Request {
                command: CommandTypes.GetCodeFixes;
                arguments: CodeFixRequestArgs;
            }
            export interface GetCombinedCodeFixRequest extends Request {
                command: CommandTypes.GetCombinedCodeFix;
                arguments: GetCombinedCodeFixRequestArgs;
            }
            export interface GetCombinedCodeFixResponse extends Response {
                body: CombinedCodeActions;
            }
            export interface ApplyCodeActionCommandRequest extends Request {
                command: CommandTypes.ApplyCodeActionCommand;
                arguments: ApplyCodeActionCommandRequestArgs;
            }
            export interface ApplyCodeActionCommandResponse extends Response {
            }
            export interface FileRangeRequestArgs extends FileRequestArgs, FileRange {
            }
            export interface CodeFixRequestArgs extends FileRangeRequestArgs {
                errorCodes: readonly number[];
            }
            export interface GetCombinedCodeFixRequestArgs {
                scope: GetCombinedCodeFixScope;
                fixId: {};
            }
            export interface GetCombinedCodeFixScope {
                type: "file";
                args: FileRequestArgs;
            }
            export interface ApplyCodeActionCommandRequestArgs {
                command: {};
            }
            export interface GetCodeFixesResponse extends Response {
                body?: CodeAction[];
            }
            export interface FileLocationRequest extends FileRequest {
                arguments: FileLocationRequestArgs;
            }
            export interface GetSupportedCodeFixesRequest extends Request {
                command: CommandTypes.GetSupportedCodeFixes;
                arguments?: Partial<FileRequestArgs>;
            }
            export interface GetSupportedCodeFixesResponse extends Response {
                body?: string[];
            }
            export interface EncodedSemanticClassificationsRequest extends FileRequest {
                arguments: EncodedSemanticClassificationsRequestArgs;
            }
            export interface EncodedSemanticClassificationsRequestArgs extends FileRequestArgs {
                start: number;
                length: number;
                format?: "original" | "2020";
            }
            export interface EncodedSemanticClassificationsResponse extends Response {
                body?: EncodedSemanticClassificationsResponseBody;
            }
            export interface EncodedSemanticClassificationsResponseBody {
                endOfLineState: EndOfLineState;
                spans: number[];
            }
            export interface DocumentHighlightsRequestArgs extends FileLocationRequestArgs {
                filesToSearch: string[];
            }
            export interface DefinitionRequest extends FileLocationRequest {
                command: CommandTypes.Definition;
            }
            export interface DefinitionAndBoundSpanRequest extends FileLocationRequest {
                readonly command: CommandTypes.DefinitionAndBoundSpan;
            }
            export interface FindSourceDefinitionRequest extends FileLocationRequest {
                readonly command: CommandTypes.FindSourceDefinition;
            }
            export interface DefinitionAndBoundSpanResponse extends Response {
                readonly body: DefinitionInfoAndBoundSpan;
            }
            export interface TypeDefinitionRequest extends FileLocationRequest {
                command: CommandTypes.TypeDefinition;
            }
            export interface ImplementationRequest extends FileLocationRequest {
                command: CommandTypes.Implementation;
            }
            export interface Location {
                line: number;
                offset: number;
            }
            export interface TextSpan {
                start: Location;
                end: Location;
            }
            export interface FileSpan extends TextSpan {
                file: string;
            }
            export interface JSDocTagInfo {
                name: string;
                text?: string | SymbolDisplayPart[];
            }
            export interface TextSpanWithContext extends TextSpan {
                contextStart?: Location;
                contextEnd?: Location;
            }
            export interface FileSpanWithContext extends FileSpan, TextSpanWithContext {
            }
            export interface DefinitionInfo extends FileSpanWithContext {
                unverified?: boolean;
            }
            export interface DefinitionInfoAndBoundSpan {
                definitions: readonly DefinitionInfo[];
                textSpan: TextSpan;
            }
            export interface DefinitionResponse extends Response {
                body?: DefinitionInfo[];
            }
            export interface DefinitionInfoAndBoundSpanResponse extends Response {
                body?: DefinitionInfoAndBoundSpan;
            }
            export type DefinitionInfoAndBoundSpanReponse = DefinitionInfoAndBoundSpanResponse;
            export interface TypeDefinitionResponse extends Response {
                body?: FileSpanWithContext[];
            }
            export interface ImplementationResponse extends Response {
                body?: FileSpanWithContext[];
            }
            export interface BraceCompletionRequest extends FileLocationRequest {
                command: CommandTypes.BraceCompletion;
                arguments: BraceCompletionRequestArgs;
            }
            export interface BraceCompletionRequestArgs extends FileLocationRequestArgs {
                openingBrace: string;
            }
            export interface JsxClosingTagRequest extends FileLocationRequest {
                readonly command: CommandTypes.JsxClosingTag;
                readonly arguments: JsxClosingTagRequestArgs;
            }
            export interface JsxClosingTagRequestArgs extends FileLocationRequestArgs {
            }
            export interface JsxClosingTagResponse extends Response {
                readonly body: TextInsertion;
            }
            export interface LinkedEditingRangeRequest extends FileLocationRequest {
                readonly command: CommandTypes.LinkedEditingRange;
            }
            export interface LinkedEditingRangesBody {
                ranges: TextSpan[];
                wordPattern?: string;
            }
            export interface LinkedEditingRangeResponse extends Response {
                readonly body: LinkedEditingRangesBody;
            }
            export interface DocumentHighlightsRequest extends FileLocationRequest {
                command: CommandTypes.DocumentHighlights;
                arguments: DocumentHighlightsRequestArgs;
            }
            export interface HighlightSpan extends TextSpanWithContext {
                kind: HighlightSpanKind;
            }
            export interface DocumentHighlightsItem {
                file: string;
                highlightSpans: HighlightSpan[];
            }
            export interface DocumentHighlightsResponse extends Response {
                body?: DocumentHighlightsItem[];
            }
            export interface ReferencesRequest extends FileLocationRequest {
                command: CommandTypes.References;
            }
            export interface ReferencesResponseItem extends FileSpanWithContext {
                lineText?: string;
                isWriteAccess: boolean;
                isDefinition?: boolean;
            }
            export interface ReferencesResponseBody {
                refs: readonly ReferencesResponseItem[];
                symbolName: string;
                symbolStartOffset: number;
                symbolDisplayString: string;
            }
            export interface ReferencesResponse extends Response {
                body?: ReferencesResponseBody;
            }
            export interface FileReferencesRequest extends FileRequest {
                command: CommandTypes.FileReferences;
            }
            export interface FileReferencesResponseBody {
                refs: readonly ReferencesResponseItem[];
                symbolName: string;
            }
            export interface FileReferencesResponse extends Response {
                body?: FileReferencesResponseBody;
            }
            export interface RenameRequestArgs extends FileLocationRequestArgs {
                findInComments?: boolean;
                findInStrings?: boolean;
            }
            export interface RenameRequest extends FileLocationRequest {
                command: CommandTypes.Rename;
                arguments: RenameRequestArgs;
            }
            export type RenameInfo = RenameInfoSuccess | RenameInfoFailure;
            export type RenameInfoSuccess = ChangePropertyTypes<ts.RenameInfoSuccess, {
                triggerSpan: TextSpan;
            }>;
            export interface SpanGroup {
                file: string;
                locs: RenameTextSpan[];
            }
            export interface RenameTextSpan extends TextSpanWithContext {
                readonly prefixText?: string;
                readonly suffixText?: string;
            }
            export interface RenameResponseBody {
                info: RenameInfo;
                locs: readonly SpanGroup[];
            }
            export interface RenameResponse extends Response {
                body?: RenameResponseBody;
            }
            export interface ExternalFile {
                fileName: string;
                scriptKind?: ScriptKindName | ScriptKind;
                hasMixedContent?: boolean;
                content?: string;
            }
            export interface ExternalProject {
                projectFileName: string;
                rootFiles: ExternalFile[];
                options: ExternalProjectCompilerOptions;
                typeAcquisition?: TypeAcquisition;
            }
            export interface CompileOnSaveMixin {
                compileOnSave?: boolean;
            }
            export type ExternalProjectCompilerOptions = CompilerOptions & CompileOnSaveMixin & WatchOptions;
            export interface FileWithProjectReferenceRedirectInfo {
                fileName: string;
                isSourceOfProjectReferenceRedirect: boolean;
            }
            export interface ProjectChanges {
                added: string[] | FileWithProjectReferenceRedirectInfo[];
                removed: string[] | FileWithProjectReferenceRedirectInfo[];
                updated: string[] | FileWithProjectReferenceRedirectInfo[];
                updatedRedirects?: FileWithProjectReferenceRedirectInfo[];
            }
            export interface ConfigureRequestArguments {
                hostInfo?: string;
                file?: string;
                formatOptions?: FormatCodeSettings;
                preferences?: UserPreferences;
                extraFileExtensions?: FileExtensionInfo[];
                watchOptions?: WatchOptions;
            }
            export enum WatchFileKind {
                FixedPollingInterval = "FixedPollingInterval",
                PriorityPollingInterval = "PriorityPollingInterval",
                DynamicPriorityPolling = "DynamicPriorityPolling",
                FixedChunkSizePolling = "FixedChunkSizePolling",
                UseFsEvents = "UseFsEvents",
                UseFsEventsOnParentDirectory = "UseFsEventsOnParentDirectory",
            }
            export enum WatchDirectoryKind {
                UseFsEvents = "UseFsEvents",
                FixedPollingInterval = "FixedPollingInterval",
                DynamicPriorityPolling = "DynamicPriorityPolling",
                FixedChunkSizePolling = "FixedChunkSizePolling",
            }
            export enum PollingWatchKind {
                FixedInterval = "FixedInterval",
                PriorityInterval = "PriorityInterval",
                DynamicPriority = "DynamicPriority",
                FixedChunkSize = "FixedChunkSize",
            }
            export interface WatchOptions {
                watchFile?: WatchFileKind | ts.WatchFileKind;
                watchDirectory?: WatchDirectoryKind | ts.WatchDirectoryKind;
                fallbackPolling?: PollingWatchKind | ts.PollingWatchKind;
                synchronousWatchDirectory?: boolean;
                excludeDirectories?: string[];
                excludeFiles?: string[];
                [option: string]: CompilerOptionsValue | undefined;
            }
            export interface ConfigureRequest extends Request {
                command: CommandTypes.Configure;
                arguments: ConfigureRequestArguments;
            }
            export interface ConfigureResponse extends Response {
            }
            export interface ConfigurePluginRequestArguments {
                pluginName: string;
                configuration: any;
            }
            export interface ConfigurePluginRequest extends Request {
                command: CommandTypes.ConfigurePlugin;
                arguments: ConfigurePluginRequestArguments;
            }
            export interface ConfigurePluginResponse extends Response {
            }
            export interface SelectionRangeRequest extends FileRequest {
                command: CommandTypes.SelectionRange;
                arguments: SelectionRangeRequestArgs;
            }
            export interface SelectionRangeRequestArgs extends FileRequestArgs {
                locations: Location[];
            }
            export interface SelectionRangeResponse extends Response {
                body?: SelectionRange[];
            }
            export interface SelectionRange {
                textSpan: TextSpan;
                parent?: SelectionRange;
            }
            export interface ToggleLineCommentRequest extends FileRequest {
                command: CommandTypes.ToggleLineComment;
                arguments: FileRangeRequestArgs;
            }
            export interface ToggleMultilineCommentRequest extends FileRequest {
                command: CommandTypes.ToggleMultilineComment;
                arguments: FileRangeRequestArgs;
            }
            export interface CommentSelectionRequest extends FileRequest {
                command: CommandTypes.CommentSelection;
                arguments: FileRangeRequestArgs;
            }
            export interface UncommentSelectionRequest extends FileRequest {
                command: CommandTypes.UncommentSelection;
                arguments: FileRangeRequestArgs;
            }
            export interface OpenRequestArgs extends FileRequestArgs {
                fileContent?: string;
                scriptKindName?: ScriptKindName;
                projectRootPath?: string;
            }
            export type ScriptKindName = "TS" | "JS" | "TSX" | "JSX";
            export interface OpenRequest extends Request {
                command: CommandTypes.Open;
                arguments: OpenRequestArgs;
            }
            export interface OpenExternalProjectRequest extends Request {
                command: CommandTypes.OpenExternalProject;
                arguments: OpenExternalProjectArgs;
            }
            export type OpenExternalProjectArgs = ExternalProject;
            export interface OpenExternalProjectsRequest extends Request {
                command: CommandTypes.OpenExternalProjects;
                arguments: OpenExternalProjectsArgs;
            }
            export interface OpenExternalProjectsArgs {
                projects: ExternalProject[];
            }
            export interface OpenExternalProjectResponse extends Response {
            }
            export interface OpenExternalProjectsResponse extends Response {
            }
            export interface CloseExternalProjectRequest extends Request {
                command: CommandTypes.CloseExternalProject;
                arguments: CloseExternalProjectRequestArgs;
            }
            export interface CloseExternalProjectRequestArgs {
                projectFileName: string;
            }
            export interface CloseExternalProjectResponse extends Response {
            }
            export interface UpdateOpenRequest extends Request {
                command: CommandTypes.UpdateOpen;
                arguments: UpdateOpenRequestArgs;
            }
            export interface UpdateOpenRequestArgs {
                openFiles?: OpenRequestArgs[];
                changedFiles?: FileCodeEdits[];
                closedFiles?: string[];
            }
            export type InferredProjectCompilerOptions = ExternalProjectCompilerOptions & TypeAcquisition;
            export interface SetCompilerOptionsForInferredProjectsRequest extends Request {
                command: CommandTypes.CompilerOptionsForInferredProjects;
                arguments: SetCompilerOptionsForInferredProjectsArgs;
            }
            export interface SetCompilerOptionsForInferredProjectsArgs {
                options: InferredProjectCompilerOptions;
                projectRootPath?: string;
            }
            export interface SetCompilerOptionsForInferredProjectsResponse extends Response {
            }
            export interface ExitRequest extends Request {
                command: CommandTypes.Exit;
            }
            export interface CloseRequest extends FileRequest {
                command: CommandTypes.Close;
            }
            export interface WatchChangeRequest extends Request {
                command: CommandTypes.WatchChange;
                arguments: WatchChangeRequestArgs | readonly WatchChangeRequestArgs[];
            }
            export interface WatchChangeRequestArgs {
                id: number;
                created?: string[];
                deleted?: string[];
                updated?: string[];
            }
            export interface CompileOnSaveAffectedFileListRequest extends FileRequest {
                command: CommandTypes.CompileOnSaveAffectedFileList;
            }
            export interface CompileOnSaveAffectedFileListSingleProject {
                projectFileName: string;
                fileNames: string[];
                projectUsesOutFile: boolean;
            }
            export interface CompileOnSaveAffectedFileListResponse extends Response {
                body: CompileOnSaveAffectedFileListSingleProject[];
            }
            export interface CompileOnSaveEmitFileRequest extends FileRequest {
                command: CommandTypes.CompileOnSaveEmitFile;
                arguments: CompileOnSaveEmitFileRequestArgs;
            }
            export interface CompileOnSaveEmitFileRequestArgs extends FileRequestArgs {
                forced?: boolean;
                includeLinePosition?: boolean;
                richResponse?: boolean;
            }
            export interface CompileOnSaveEmitFileResponse extends Response {
                body: boolean | EmitResult;
            }
            export interface EmitResult {
                emitSkipped: boolean;
                diagnostics: Diagnostic[] | DiagnosticWithLinePosition[];
            }
            export interface QuickInfoRequest extends FileLocationRequest {
                command: CommandTypes.Quickinfo;
                arguments: FileLocationRequestArgs;
            }
            export interface QuickInfoRequestArgs extends FileLocationRequestArgs {
                verbosityLevel?: number;
            }
            export interface QuickInfoResponseBody {
                kind: ScriptElementKind;
                kindModifiers: string;
                start: Location;
                end: Location;
                displayString: string;
                documentation: string | SymbolDisplayPart[];
                tags: JSDocTagInfo[];
                canIncreaseVerbosityLevel?: boolean;
            }
            export interface QuickInfoResponse extends Response {
                body?: QuickInfoResponseBody;
            }
            export interface FormatRequestArgs extends FileLocationRequestArgs {
                endLine: number;
                endOffset: number;
                options?: FormatCodeSettings;
            }
            export interface FormatRequest extends FileLocationRequest {
                command: CommandTypes.Format;
                arguments: FormatRequestArgs;
            }
            export interface CodeEdit {
                start: Location;
                end: Location;
                newText: string;
            }
            export interface FileCodeEdits {
                fileName: string;
                textChanges: CodeEdit[];
            }
            export interface CodeFixResponse extends Response {
                body?: CodeFixAction[];
            }
            export interface CodeAction {
                description: string;
                changes: FileCodeEdits[];
                commands?: {}[];
            }
            export interface CombinedCodeActions {
                changes: readonly FileCodeEdits[];
                commands?: readonly {}[];
            }
            export interface CodeFixAction extends CodeAction {
                fixName: string;
                fixId?: {};
                fixAllDescription?: string;
            }
            export interface FormatResponse extends Response {
                body?: CodeEdit[];
            }
            export interface FormatOnKeyRequestArgs extends FileLocationRequestArgs {
                key: string;
                options?: FormatCodeSettings;
            }
            export interface FormatOnKeyRequest extends FileLocationRequest {
                command: CommandTypes.Formatonkey;
                arguments: FormatOnKeyRequestArgs;
            }
            export interface CompletionsRequestArgs extends FileLocationRequestArgs {
                prefix?: string;
                triggerCharacter?: CompletionsTriggerCharacter;
                triggerKind?: CompletionTriggerKind;
                includeExternalModuleExports?: boolean;
                includeInsertTextCompletions?: boolean;
            }
            export interface CompletionsRequest extends FileLocationRequest {
                command: CommandTypes.Completions | CommandTypes.CompletionInfo;
                arguments: CompletionsRequestArgs;
            }
            export interface CompletionDetailsRequestArgs extends FileLocationRequestArgs {
                entryNames: (string | CompletionEntryIdentifier)[];
            }
            export interface CompletionEntryIdentifier {
                name: string;
                source?: string;
                data?: unknown;
            }
            export interface CompletionDetailsRequest extends FileLocationRequest {
                command: CommandTypes.CompletionDetails;
                arguments: CompletionDetailsRequestArgs;
            }
            export interface JSDocLinkDisplayPart extends SymbolDisplayPart {
                target: FileSpan;
            }
            export type CompletionEntry = ChangePropertyTypes<Omit<ts.CompletionEntry, "symbol">, {
                replacementSpan: TextSpan;
                data: unknown;
            }>;
            export type CompletionEntryDetails = ChangePropertyTypes<ts.CompletionEntryDetails, {
                tags: JSDocTagInfo[];
                codeActions: CodeAction[];
            }>;
            export interface CompletionsResponse extends Response {
                body?: CompletionEntry[];
            }
            export interface CompletionInfoResponse extends Response {
                body?: CompletionInfo;
            }
            export type CompletionInfo = ChangePropertyTypes<ts.CompletionInfo, {
                entries: readonly CompletionEntry[];
                optionalReplacementSpan: TextSpan;
            }>;
            export interface CompletionDetailsResponse extends Response {
                body?: CompletionEntryDetails[];
            }
            export type SignatureHelpItem = ChangePropertyTypes<ts.SignatureHelpItem, {
                tags: JSDocTagInfo[];
            }>;
            export interface SignatureHelpItems {
                items: SignatureHelpItem[];
                applicableSpan: TextSpan;
                selectedItemIndex: number;
                argumentIndex: number;
                argumentCount: number;
            }
            export interface SignatureHelpRequestArgs extends FileLocationRequestArgs {
                triggerReason?: SignatureHelpTriggerReason;
            }
            export interface SignatureHelpRequest extends FileLocationRequest {
                command: CommandTypes.SignatureHelp;
                arguments: SignatureHelpRequestArgs;
            }
            export interface SignatureHelpResponse extends Response {
                body?: SignatureHelpItems;
            }
            export interface InlayHintsRequestArgs extends FileRequestArgs {
                start: number;
                length: number;
            }
            export interface InlayHintsRequest extends Request {
                command: CommandTypes.ProvideInlayHints;
                arguments: InlayHintsRequestArgs;
            }
            export type InlayHintItem = ChangePropertyTypes<ts.InlayHint, {
                position: Location;
                displayParts: InlayHintItemDisplayPart[];
            }>;
            export interface InlayHintItemDisplayPart {
                text: string;
                span?: FileSpan;
            }
            export interface InlayHintsResponse extends Response {
                body?: InlayHintItem[];
            }
            export interface MapCodeRequestArgs extends FileRequestArgs {
                mapping: MapCodeRequestDocumentMapping;
            }
            export interface MapCodeRequestDocumentMapping {
                contents: string[];
                focusLocations?: TextSpan[][];
            }
            export interface MapCodeRequest extends FileRequest {
                command: CommandTypes.MapCode;
                arguments: MapCodeRequestArgs;
            }
            export interface MapCodeResponse extends Response {
                body: readonly FileCodeEdits[];
            }
            export interface SemanticDiagnosticsSyncRequest extends FileRequest {
                command: CommandTypes.SemanticDiagnosticsSync;
                arguments: SemanticDiagnosticsSyncRequestArgs;
            }
            export interface SemanticDiagnosticsSyncRequestArgs extends FileRequestArgs {
                includeLinePosition?: boolean;
            }
            export interface SemanticDiagnosticsSyncResponse extends Response {
                body?: Diagnostic[] | DiagnosticWithLinePosition[];
            }
            export interface SuggestionDiagnosticsSyncRequest extends FileRequest {
                command: CommandTypes.SuggestionDiagnosticsSync;
                arguments: SuggestionDiagnosticsSyncRequestArgs;
            }
            export type SuggestionDiagnosticsSyncRequestArgs = SemanticDiagnosticsSyncRequestArgs;
            export type SuggestionDiagnosticsSyncResponse = SemanticDiagnosticsSyncResponse;
            export interface SyntacticDiagnosticsSyncRequest extends FileRequest {
                command: CommandTypes.SyntacticDiagnosticsSync;
                arguments: SyntacticDiagnosticsSyncRequestArgs;
            }
            export interface SyntacticDiagnosticsSyncRequestArgs extends FileRequestArgs {
                includeLinePosition?: boolean;
            }
            export interface SyntacticDiagnosticsSyncResponse extends Response {
                body?: Diagnostic[] | DiagnosticWithLinePosition[];
            }
            export interface GeterrForProjectRequestArgs {
                file: string;
                delay: number;
            }
            export interface GeterrForProjectRequest extends Request {
                command: CommandTypes.GeterrForProject;
                arguments: GeterrForProjectRequestArgs;
            }
            export interface GeterrRequestArgs {
                files: (string | FileRangesRequestArgs)[];
                delay: number;
            }
            export interface GeterrRequest extends Request {
                command: CommandTypes.Geterr;
                arguments: GeterrRequestArgs;
            }
            export interface FileRange {
                startLine: number;
                startOffset: number;
                endLine: number;
                endOffset: number;
            }
            export interface FileRangesRequestArgs extends Pick<FileRequestArgs, "file"> {
                ranges: FileRange[];
            }
            export type RequestCompletedEventName = "requestCompleted";
            export interface RequestCompletedEvent extends Event {
                event: RequestCompletedEventName;
                body: RequestCompletedEventBody;
            }
            export interface RequestCompletedEventBody {
                request_seq: number;
                performanceData?: PerformanceData;
            }
            export interface Diagnostic {
                start: Location;
                end: Location;
                text: string;
                category: string;
                reportsUnnecessary?: {};
                reportsDeprecated?: {};
                relatedInformation?: DiagnosticRelatedInformation[];
                code?: number;
                source?: string;
            }
            export interface DiagnosticWithFileName extends Diagnostic {
                fileName: string;
            }
            export interface DiagnosticRelatedInformation {
                category: string;
                code: number;
                message: string;
                span?: FileSpan;
            }
            export interface DiagnosticEventBody {
                file: string;
                diagnostics: Diagnostic[];
                spans?: TextSpan[];
            }
            export type DiagnosticEventKind = "semanticDiag" | "syntaxDiag" | "suggestionDiag" | "regionSemanticDiag";
            export interface DiagnosticEvent extends Event {
                body?: DiagnosticEventBody;
                event: DiagnosticEventKind;
            }
            export interface ConfigFileDiagnosticEventBody {
                triggerFile: string;
                configFile: string;
                diagnostics: DiagnosticWithFileName[];
            }
            export interface ConfigFileDiagnosticEvent extends Event {
                body?: ConfigFileDiagnosticEventBody;
                event: "configFileDiag";
            }
            export type ProjectLanguageServiceStateEventName = "projectLanguageServiceState";
            export interface ProjectLanguageServiceStateEvent extends Event {
                event: ProjectLanguageServiceStateEventName;
                body?: ProjectLanguageServiceStateEventBody;
            }
            export interface ProjectLanguageServiceStateEventBody {
                projectName: string;
                languageServiceEnabled: boolean;
            }
            export type ProjectsUpdatedInBackgroundEventName = "projectsUpdatedInBackground";
            export interface ProjectsUpdatedInBackgroundEvent extends Event {
                event: ProjectsUpdatedInBackgroundEventName;
                body: ProjectsUpdatedInBackgroundEventBody;
            }
            export interface ProjectsUpdatedInBackgroundEventBody {
                openFiles: string[];
            }
            export type ProjectLoadingStartEventName = "projectLoadingStart";
            export interface ProjectLoadingStartEvent extends Event {
                event: ProjectLoadingStartEventName;
                body: ProjectLoadingStartEventBody;
            }
            export interface ProjectLoadingStartEventBody {
                projectName: string;
                reason: string;
            }
            export type ProjectLoadingFinishEventName = "projectLoadingFinish";
            export interface ProjectLoadingFinishEvent extends Event {
                event: ProjectLoadingFinishEventName;
                body: ProjectLoadingFinishEventBody;
            }
            export interface ProjectLoadingFinishEventBody {
                projectName: string;
            }
            export type SurveyReadyEventName = "surveyReady";
            export interface SurveyReadyEvent extends Event {
                event: SurveyReadyEventName;
                body: SurveyReadyEventBody;
            }
            export interface SurveyReadyEventBody {
                surveyId: string;
            }
            export type LargeFileReferencedEventName = "largeFileReferenced";
            export interface LargeFileReferencedEvent extends Event {
                event: LargeFileReferencedEventName;
                body: LargeFileReferencedEventBody;
            }
            export interface LargeFileReferencedEventBody {
                file: string;
                fileSize: number;
                maxFileSize: number;
            }
            export type CreateFileWatcherEventName = "createFileWatcher";
            export interface CreateFileWatcherEvent extends Event {
                readonly event: CreateFileWatcherEventName;
                readonly body: CreateFileWatcherEventBody;
            }
            export interface CreateFileWatcherEventBody {
                readonly id: number;
                readonly path: string;
            }
            export type CreateDirectoryWatcherEventName = "createDirectoryWatcher";
            export interface CreateDirectoryWatcherEvent extends Event {
                readonly event: CreateDirectoryWatcherEventName;
                readonly body: CreateDirectoryWatcherEventBody;
            }
            export interface CreateDirectoryWatcherEventBody {
                readonly id: number;
                readonly path: string;
                readonly recursive: boolean;
                readonly ignoreUpdate?: boolean;
            }
            export type CloseFileWatcherEventName = "closeFileWatcher";
            export interface CloseFileWatcherEvent extends Event {
                readonly event: CloseFileWatcherEventName;
                readonly body: CloseFileWatcherEventBody;
            }
            export interface CloseFileWatcherEventBody {
                readonly id: number;
            }
            export interface ReloadRequestArgs extends FileRequestArgs {
                tmpfile: string;
            }
            export interface ReloadRequest extends FileRequest {
                command: CommandTypes.Reload;
                arguments: ReloadRequestArgs;
            }
            export interface ReloadResponse extends Response {
            }
            export interface SavetoRequestArgs extends FileRequestArgs {
                tmpfile: string;
            }
            export interface SavetoRequest extends FileRequest {
                command: CommandTypes.Saveto;
                arguments: SavetoRequestArgs;
            }
            export interface NavtoRequestArgs {
                searchValue: string;
                maxResultCount?: number;
                file?: string;
                currentFileOnly?: boolean;
                projectFileName?: string;
            }
            export interface NavtoRequest extends Request {
                command: CommandTypes.Navto;
                arguments: NavtoRequestArgs;
            }
            export interface NavtoItem extends FileSpan {
                name: string;
                kind: ScriptElementKind;
                matchKind: string;
                isCaseSensitive: boolean;
                kindModifiers?: string;
                containerName?: string;
                containerKind?: ScriptElementKind;
            }
            export interface NavtoResponse extends Response {
                body?: NavtoItem[];
            }
            export interface ChangeRequestArgs extends FormatRequestArgs {
                insertString?: string;
            }
            export interface ChangeRequest extends FileLocationRequest {
                command: CommandTypes.Change;
                arguments: ChangeRequestArgs;
            }
            export interface BraceResponse extends Response {
                body?: TextSpan[];
            }
            export interface BraceRequest extends FileLocationRequest {
                command: CommandTypes.Brace;
            }
            export interface NavBarRequest extends FileRequest {
                command: CommandTypes.NavBar;
            }
            export interface NavTreeRequest extends FileRequest {
                command: CommandTypes.NavTree;
            }
            export interface NavigationBarItem {
                text: string;
                kind: ScriptElementKind;
                kindModifiers?: string;
                spans: TextSpan[];
                childItems?: NavigationBarItem[];
                indent: number;
            }
            export interface NavigationTree {
                text: string;
                kind: ScriptElementKind;
                kindModifiers: string;
                spans: TextSpan[];
                nameSpan: TextSpan | undefined;
                childItems?: NavigationTree[];
            }
            export type TelemetryEventName = "telemetry";
            export interface TelemetryEvent extends Event {
                event: TelemetryEventName;
                body: TelemetryEventBody;
            }
            export interface TelemetryEventBody {
                telemetryEventName: string;
                payload: any;
            }
            export type TypesInstallerInitializationFailedEventName = "typesInstallerInitializationFailed";
            export interface TypesInstallerInitializationFailedEvent extends Event {
                event: TypesInstallerInitializationFailedEventName;
                body: TypesInstallerInitializationFailedEventBody;
            }
            export interface TypesInstallerInitializationFailedEventBody {
                message: string;
            }
            export type TypingsInstalledTelemetryEventName = "typingsInstalled";
            export interface TypingsInstalledTelemetryEventBody extends TelemetryEventBody {
                telemetryEventName: TypingsInstalledTelemetryEventName;
                payload: TypingsInstalledTelemetryEventPayload;
            }
            export interface TypingsInstalledTelemetryEventPayload {
                installedPackages: string;
                installSuccess: boolean;
                typingsInstallerVersion: string;
            }
            export type BeginInstallTypesEventName = "beginInstallTypes";
            export type EndInstallTypesEventName = "endInstallTypes";
            export interface BeginInstallTypesEvent extends Event {
                event: BeginInstallTypesEventName;
                body: BeginInstallTypesEventBody;
            }
            export interface EndInstallTypesEvent extends Event {
                event: EndInstallTypesEventName;
                body: EndInstallTypesEventBody;
            }
            export interface InstallTypesEventBody {
                eventId: number;
                packages: readonly string[];
            }
            export interface BeginInstallTypesEventBody extends InstallTypesEventBody {
            }
            export interface EndInstallTypesEventBody extends InstallTypesEventBody {
                success: boolean;
            }
            export interface NavBarResponse extends Response {
                body?: NavigationBarItem[];
            }
            export interface NavTreeResponse extends Response {
                body?: NavigationTree;
            }
            export type CallHierarchyItem = ChangePropertyTypes<ts.CallHierarchyItem, {
                span: TextSpan;
                selectionSpan: TextSpan;
            }>;
            export interface CallHierarchyIncomingCall {
                from: CallHierarchyItem;
                fromSpans: TextSpan[];
            }
            export interface CallHierarchyOutgoingCall {
                to: CallHierarchyItem;
                fromSpans: TextSpan[];
            }
            export interface PrepareCallHierarchyRequest extends FileLocationRequest {
                command: CommandTypes.PrepareCallHierarchy;
            }
            export interface PrepareCallHierarchyResponse extends Response {
                readonly body: CallHierarchyItem | CallHierarchyItem[];
            }
            export interface ProvideCallHierarchyIncomingCallsRequest extends FileLocationRequest {
                command: CommandTypes.ProvideCallHierarchyIncomingCalls;
            }
            export interface ProvideCallHierarchyIncomingCallsResponse extends Response {
                readonly body: CallHierarchyIncomingCall[];
            }
            export interface ProvideCallHierarchyOutgoingCallsRequest extends FileLocationRequest {
                command: CommandTypes.ProvideCallHierarchyOutgoingCalls;
            }
            export interface ProvideCallHierarchyOutgoingCallsResponse extends Response {
                readonly body: CallHierarchyOutgoingCall[];
            }
            export enum IndentStyle {
                None = "None",
                Block = "Block",
                Smart = "Smart",
            }
            export type EditorSettings = ChangePropertyTypes<ts.EditorSettings, {
                indentStyle: IndentStyle | ts.IndentStyle;
            }>;
            export type FormatCodeSettings = ChangePropertyTypes<ts.FormatCodeSettings, {
                indentStyle: IndentStyle | ts.IndentStyle;
            }>;
            export type CompilerOptions = ChangePropertyTypes<ChangeStringIndexSignature<ts.CompilerOptions, CompilerOptionsValue>, {
                jsx: JsxEmit | ts.JsxEmit;
                module: ModuleKind | ts.ModuleKind;
                moduleResolution: ModuleResolutionKind | ts.ModuleResolutionKind;
                newLine: NewLineKind | ts.NewLineKind;
                target: ScriptTarget | ts.ScriptTarget;
            }>;
            export enum JsxEmit {
                None = "none",
                Preserve = "preserve",
                ReactNative = "react-native",
                React = "react",
                ReactJSX = "react-jsx",
                ReactJSXDev = "react-jsxdev",
            }
            export enum ModuleKind {
                None = "none",
                CommonJS = "commonjs",
                AMD = "amd",
                UMD = "umd",
                System = "system",
                ES6 = "es6",
                ES2015 = "es2015",
                ES2020 = "es2020",
                ES2022 = "es2022",
                ESNext = "esnext",
                Node16 = "node16",
                Node18 = "node18",
                Node20 = "node20",
                NodeNext = "nodenext",
                Preserve = "preserve",
            }
            export enum ModuleResolutionKind {
                Classic = "classic",
                Node = "node",
                NodeJs = "node",
                Node10 = "node10",
                Node16 = "node16",
                NodeNext = "nodenext",
                Bundler = "bundler",
            }
            export enum NewLineKind {
                Crlf = "Crlf",
                Lf = "Lf",
            }
            export enum ScriptTarget {
                ES3 = "es3",
                ES5 = "es5",
                ES6 = "es6",
                ES2015 = "es2015",
                ES2016 = "es2016",
                ES2017 = "es2017",
                ES2018 = "es2018",
                ES2019 = "es2019",
                ES2020 = "es2020",
                ES2021 = "es2021",
                ES2022 = "es2022",
                ES2023 = "es2023",
                ES2024 = "es2024",
                ESNext = "esnext",
                JSON = "json",
                Latest = "esnext",
            }
        }
        namespace typingsInstaller {
            interface Log {
                isEnabled(): boolean;
                writeLine(text: string): void;
            }
            type RequestCompletedAction = (success: boolean) => void;
            interface PendingRequest {
                requestId: number;
                packageNames: string[];
                cwd: string;
                onRequestCompleted: RequestCompletedAction;
            }
            abstract class TypingsInstaller {
                protected readonly installTypingHost: InstallTypingHost;
                private readonly globalCachePath;
                private readonly safeListPath;
                private readonly typesMapLocation;
                private readonly throttleLimit;
                protected readonly log: Log;
                private readonly packageNameToTypingLocation;
                private readonly missingTypingsSet;
                private readonly knownCachesSet;
                private readonly projectWatchers;
                private safeList;
                private pendingRunRequests;
                private installRunCount;
                private inFlightRequestCount;
                abstract readonly typesRegistry: Map<string, MapLike<string>>;
                constructor(installTypingHost: InstallTypingHost, globalCachePath: string, safeListPath: Path, typesMapLocation: Path, throttleLimit: number, log?: Log);
                closeProject(req: CloseProject): void;
                private closeWatchers;
                install(req: DiscoverTypings): void;
                private initializeSafeList;
                private processCacheLocation;
                private filterTypings;
                protected ensurePackageDirectoryExists(directory: string): void;
                private installTypings;
                private ensureDirectoryExists;
                private watchFiles;
                private createSetTypings;
                private installTypingsAsync;
                private executeWithThrottling;
                protected abstract installWorker(requestId: number, packageNames: string[], cwd: string, onRequestCompleted: RequestCompletedAction): void;
                protected abstract sendResponse(response: SetTypings | InvalidateCachedTypings | BeginInstallTypes | EndInstallTypes | WatchTypingLocations): void;
                protected readonly latestDistTag = "latest";
            }
        }
        type ActionSet = "action::set";
        type ActionInvalidate = "action::invalidate";
        type ActionPackageInstalled = "action::packageInstalled";
        type EventTypesRegistry = "event::typesRegistry";
        type EventBeginInstallTypes = "event::beginInstallTypes";
        type EventEndInstallTypes = "event::endInstallTypes";
        type EventInitializationFailed = "event::initializationFailed";
        type ActionWatchTypingLocations = "action::watchTypingLocations";
        interface TypingInstallerResponse {
            readonly kind: ActionSet | ActionInvalidate | EventTypesRegistry | ActionPackageInstalled | EventBeginInstallTypes | EventEndInstallTypes | EventInitializationFailed | ActionWatchTypingLocations;
        }
        interface TypingInstallerRequestWithProjectName {
            readonly projectName: string;
        }
        interface DiscoverTypings extends TypingInstallerRequestWithProjectName {
            readonly fileNames: string[];
            readonly projectRootPath: Path;
            readonly compilerOptions: CompilerOptions;
            readonly typeAcquisition: TypeAcquisition;
            readonly unresolvedImports: SortedReadonlyArray<string>;
            readonly cachePath?: string;
            readonly kind: "discover";
        }
        interface CloseProject extends TypingInstallerRequestWithProjectName {
            readonly kind: "closeProject";
        }
        interface TypesRegistryRequest {
            readonly kind: "typesRegistry";
        }
        interface InstallPackageRequest extends TypingInstallerRequestWithProjectName {
            readonly kind: "installPackage";
            readonly fileName: Path;
            readonly packageName: string;
            readonly projectRootPath: Path;
            readonly id: number;
        }
        interface PackageInstalledResponse extends ProjectResponse {
            readonly kind: ActionPackageInstalled;
            readonly id: number;
            readonly success: boolean;
            readonly message: string;
        }
        interface InitializationFailedResponse extends TypingInstallerResponse {
            readonly kind: EventInitializationFailed;
            readonly message: string;
            readonly stack?: string;
        }
        interface ProjectResponse extends TypingInstallerResponse {
            readonly projectName: string;
        }
        interface InvalidateCachedTypings extends ProjectResponse {
            readonly kind: ActionInvalidate;
        }
        interface InstallTypes extends ProjectResponse {
            readonly kind: EventBeginInstallTypes | EventEndInstallTypes;
            readonly eventId: number;
            readonly typingsInstallerVersion: string;
            readonly packagesToInstall: readonly string[];
        }
        interface BeginInstallTypes extends InstallTypes {
            readonly kind: EventBeginInstallTypes;
        }
        interface EndInstallTypes extends InstallTypes {
            readonly kind: EventEndInstallTypes;
            readonly installSuccess: boolean;
        }
        interface InstallTypingHost extends JsTyping.TypingResolutionHost {
            useCaseSensitiveFileNames: boolean;
            writeFile(path: string, content: string): void;
            createDirectory(path: string): void;
            getCurrentDirectory?(): string;
        }
        interface SetTypings extends ProjectResponse {
            readonly typeAcquisition: TypeAcquisition;
            readonly compilerOptions: CompilerOptions;
            readonly typings: string[];
            readonly unresolvedImports: SortedReadonlyArray<string>;
            readonly kind: ActionSet;
        }
        interface WatchTypingLocations extends ProjectResponse {
            readonly files: readonly string[] | undefined;
            readonly kind: ActionWatchTypingLocations;
        }
        interface CompressedData {
            length: number;
            compressionKind: string;
            data: any;
        }
        type ModuleImportResult = {
            module: {};
            error: undefined;
        } | {
            module: undefined;
            error: {
                stack?: string;
                message?: string;
            };
        };
        type RequireResult = ModuleImportResult;
        interface ServerHost extends System {
            watchFile(path: string, callback: FileWatcherCallback, pollingInterval?: number, options?: WatchOptions): FileWatcher;
            watchDirectory(path: string, callback: DirectoryWatcherCallback, recursive?: boolean, options?: WatchOptions): FileWatcher;
            preferNonRecursiveWatch?: boolean;
            setTimeout(callback: (...args: any[]) => void, ms: number, ...args: any[]): any;
            clearTimeout(timeoutId: any): void;
            setImmediate(callback: (...args: any[]) => void, ...args: any[]): any;
            clearImmediate(timeoutId: any): void;
            gc?(): void;
            trace?(s: string): void;
            require?(initialPath: string, moduleName: string): ModuleImportResult;
        }
        interface InstallPackageOptionsWithProject extends InstallPackageOptions {
            projectName: string;
            projectRootPath: Path;
        }
        interface ITypingsInstaller {
            isKnownTypesPackageName(name: string): boolean;
            installPackage(options: InstallPackageOptionsWithProject): Promise<ApplyCodeActionCommandResult>;
            enqueueInstallTypingsRequest(p: Project, typeAcquisition: TypeAcquisition, unresolvedImports: SortedReadonlyArray<string> | undefined): void;
            attach(projectService: ProjectService): void;
            onProjectClosed(p: Project): void;
            readonly globalTypingsCacheLocation: string | undefined;
        }
        function createInstallTypingsRequest(project: Project, typeAcquisition: TypeAcquisition, unresolvedImports: SortedReadonlyArray<string>, cachePath?: string): DiscoverTypings;
        function toNormalizedPath(fileName: string): NormalizedPath;
        function normalizedPathToPath(normalizedPath: NormalizedPath, currentDirectory: string, getCanonicalFileName: (f: string) => string): Path;
        function asNormalizedPath(fileName: string): NormalizedPath;
        function createNormalizedPathMap<T>(): NormalizedPathMap<T>;
        function isInferredProjectName(name: string): boolean;
        function makeInferredProjectName(counter: number): string;
        function createSortedArray<T>(): SortedArray<T>;
        enum LogLevel {
            terse = 0,
            normal = 1,
            requestTime = 2,
            verbose = 3,
        }
        const emptyArray: SortedReadonlyArray<never>;
        interface Logger {
            close(): void;
            hasLevel(level: LogLevel): boolean;
            loggingEnabled(): boolean;
            perftrc(s: string): void;
            info(s: string): void;
            startGroup(): void;
            endGroup(): void;
            msg(s: string, type?: Msg): void;
            getLogFileName(): string | undefined;
        }
        enum Msg {
            Err = "Err",
            Info = "Info",
            Perf = "Perf",
        }
        namespace Errors {
            function ThrowNoProject(): never;
            function ThrowProjectLanguageServiceDisabled(): never;
            function ThrowProjectDoesNotContainDocument(fileName: string, project: Project): never;
        }
        type NormalizedPath = string & {
            __normalizedPathTag: any;
        };
        interface NormalizedPathMap<T> {
            get(path: NormalizedPath): T | undefined;
            set(path: NormalizedPath, value: T): void;
            contains(path: NormalizedPath): boolean;
            remove(path: NormalizedPath): void;
        }
        function isDynamicFileName(fileName: NormalizedPath): boolean;
        class ScriptInfo {
            private readonly host;
            readonly fileName: NormalizedPath;
            readonly scriptKind: ScriptKind;
            readonly hasMixedContent: boolean;
            readonly path: Path;
            readonly containingProjects: Project[];
            private formatSettings;
            private preferences;
            private realpath;
            constructor(host: ServerHost, fileName: NormalizedPath, scriptKind: ScriptKind, hasMixedContent: boolean, path: Path, initialVersion?: number);
            isScriptOpen(): boolean;
            open(newText: string | undefined): void;
            close(fileExists?: boolean): void;
            getSnapshot(): IScriptSnapshot;
            private ensureRealPath;
            getFormatCodeSettings(): FormatCodeSettings | undefined;
            getPreferences(): protocol.UserPreferences | undefined;
            attachToProject(project: Project): boolean;
            isAttached(project: Project): boolean;
            detachFromProject(project: Project): void;
            detachAllProjects(): void;
            getDefaultProject(): Project;
            registerFileUpdate(): void;
            setOptions(formatSettings: FormatCodeSettings, preferences: protocol.UserPreferences | undefined): void;
            getLatestVersion(): string;
            saveTo(fileName: string): void;
            reloadFromFile(tempFileName?: NormalizedPath): boolean;
            editContent(start: number, end: number, newText: string): void;
            markContainingProjectsAsDirty(): void;
            isOrphan(): boolean;
            lineToTextSpan(line: number): TextSpan;
            lineOffsetToPosition(line: number, offset: number): number;
            positionToLineOffset(position: number): protocol.Location;
            isJavaScript(): boolean;
        }
        function allRootFilesAreJsOrDts(project: Project): boolean;
        function allFilesAreJsOrDts(project: Project): boolean;
        enum ProjectKind {
            Inferred = 0,
            Configured = 1,
            External = 2,
            AutoImportProvider = 3,
            Auxiliary = 4,
        }
        interface PluginCreateInfo {
            project: Project;
            languageService: LanguageService;
            languageServiceHost: LanguageServiceHost;
            serverHost: ServerHost;
            session?: Session<unknown>;
            config: any;
        }
        interface PluginModule {
            create(createInfo: PluginCreateInfo): LanguageService;
            getExternalFiles?(proj: Project, updateLevel: ProgramUpdateLevel): string[];
            onConfigurationChanged?(config: any): void;
        }
        interface PluginModuleWithName {
            name: string;
            module: PluginModule;
        }
        type PluginModuleFactory = (mod: {
            typescript: typeof ts;
        }) => PluginModule;
        abstract class Project implements LanguageServiceHost, ModuleResolutionHost {
            readonly projectKind: ProjectKind;
            readonly projectService: ProjectService;
            private compilerOptions;
            compileOnSaveEnabled: boolean;
            protected watchOptions: WatchOptions | undefined;
            private rootFilesMap;
            private program;
            private externalFiles;
            private missingFilesMap;
            private generatedFilesMap;
            private hasAddedorRemovedFiles;
            private hasAddedOrRemovedSymlinks;
            protected languageService: LanguageService;
            languageServiceEnabled: boolean;
            readonly trace?: (s: string) => void;
            readonly realpath?: (path: string) => string;
            private builderState;
            private updatedFileNames;
            private lastReportedFileNames;
            private lastReportedVersion;
            protected projectErrors: Diagnostic[] | undefined;
            private typingsCache;
            private typingWatchers;
            private readonly cancellationToken;
            isNonTsProject(): boolean;
            isJsOnlyProject(): boolean;
            static resolveModule(moduleName: string, initialDir: string, host: ServerHost, log: (message: string) => void): {} | undefined;
            private exportMapCache;
            private changedFilesForExportMapCache;
            private moduleSpecifierCache;
            private symlinks;
            readonly jsDocParsingMode: JSDocParsingMode | undefined;
            isKnownTypesPackageName(name: string): boolean;
            installPackage(options: InstallPackageOptions): Promise<ApplyCodeActionCommandResult>;
            getCompilationSettings(): CompilerOptions;
            getCompilerOptions(): CompilerOptions;
            getNewLine(): string;
            getProjectVersion(): string;
            getProjectReferences(): readonly ProjectReference[] | undefined;
            getScriptFileNames(): string[];
            private getOrCreateScriptInfoAndAttachToProject;
            getScriptKind(fileName: string): ScriptKind;
            getScriptVersion(filename: string): string;
            getScriptSnapshot(filename: string): IScriptSnapshot | undefined;
            getCancellationToken(): HostCancellationToken;
            getCurrentDirectory(): string;
            getDefaultLibFileName(): string;
            useCaseSensitiveFileNames(): boolean;
            readDirectory(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[];
            readFile(fileName: string): string | undefined;
            writeFile(fileName: string, content: string): void;
            fileExists(file: string): boolean;
            directoryExists(path: string): boolean;
            getDirectories(path: string): string[];
            log(s: string): void;
            error(s: string): void;
            private setInternalCompilerOptionsForEmittingJsFiles;
            getGlobalProjectErrors(): readonly Diagnostic[];
            getAllProjectErrors(): readonly Diagnostic[];
            setProjectErrors(projectErrors: Diagnostic[] | undefined): void;
            getLanguageService(ensureSynchronized?: boolean): LanguageService;
            getCompileOnSaveAffectedFileList(scriptInfo: ScriptInfo): string[];
            emitFile(scriptInfo: ScriptInfo, writeFile: (path: string, data: string, writeByteOrderMark?: boolean) => void): EmitResult;
            enableLanguageService(): void;
            disableLanguageService(lastFileExceededProgramSize?: string): void;
            getProjectName(): string;
            protected removeLocalTypingsFromTypeAcquisition(newTypeAcquisition: TypeAcquisition): TypeAcquisition;
            getExternalFiles(updateLevel?: ProgramUpdateLevel): SortedReadonlyArray<string>;
            getSourceFile(path: Path): SourceFile | undefined;
            close(): void;
            private detachScriptInfoIfNotRoot;
            isClosed(): boolean;
            hasRoots(): boolean;
            getRootFiles(): NormalizedPath[];
            getRootScriptInfos(): ScriptInfo[];
            getScriptInfos(): ScriptInfo[];
            getExcludedFiles(): readonly NormalizedPath[];
            getFileNames(excludeFilesFromExternalLibraries?: boolean, excludeConfigFiles?: boolean): NormalizedPath[];
            hasConfigFile(configFilePath: NormalizedPath): boolean;
            containsScriptInfo(info: ScriptInfo): boolean;
            containsFile(filename: NormalizedPath, requireOpen?: boolean): boolean;
            isRoot(info: ScriptInfo): boolean;
            addRoot(info: ScriptInfo, fileName?: NormalizedPath): void;
            addMissingFileRoot(fileName: NormalizedPath): void;
            removeFile(info: ScriptInfo, fileExists: boolean, detachFromProject: boolean): void;
            registerFileUpdate(fileName: string): void;
            updateGraph(): boolean;
            private closeWatchingTypingLocations;
            private onTypingInstallerWatchInvoke;
            protected removeExistingTypings(include: string[]): string[];
            private updateGraphWorker;
            private detachScriptInfoFromProject;
            private addMissingFileWatcher;
            private isWatchedMissingFile;
            private createGeneratedFileWatcher;
            private isValidGeneratedFileWatcher;
            private clearGeneratedFileWatch;
            getScriptInfoForNormalizedPath(fileName: NormalizedPath): ScriptInfo | undefined;
            getScriptInfo(uncheckedFileName: string): ScriptInfo | undefined;
            filesToString(writeProjectFileNames: boolean): string;
            private filesToStringWorker;
            setCompilerOptions(compilerOptions: CompilerOptions): void;
            setTypeAcquisition(newTypeAcquisition: TypeAcquisition | undefined): void;
            getTypeAcquisition(): TypeAcquisition;
            protected removeRoot(info: ScriptInfo): void;
            protected enableGlobalPlugins(options: CompilerOptions): void;
            protected enablePlugin(pluginConfigEntry: PluginImport, searchPaths: string[]): void;
            refreshDiagnostics(): void;
            private isDefaultProjectForOpenFiles;
        }
        class InferredProject extends Project {
            private _isJsInferredProject;
            toggleJsInferredProject(isJsInferredProject: boolean): void;
            setCompilerOptions(options?: CompilerOptions): void;
            readonly projectRootPath: string | undefined;
            addRoot(info: ScriptInfo): void;
            removeRoot(info: ScriptInfo): void;
            isProjectWithSingleRoot(): boolean;
            close(): void;
            getTypeAcquisition(): TypeAcquisition;
        }
        class AutoImportProviderProject extends Project {
            private hostProject;
            private static readonly maxDependencies;
            private rootFileNames;
            updateGraph(): boolean;
            hasRoots(): boolean;
            getScriptFileNames(): string[];
            getLanguageService(): never;
            getHostForAutoImportProvider(): never;
            getProjectReferences(): readonly ProjectReference[] | undefined;
        }
        class ConfiguredProject extends Project {
            readonly canonicalConfigFilePath: NormalizedPath;
            private projectReferences;
            private compilerHost?;
            private releaseParsedConfig;
            updateGraph(): boolean;
            getConfigFilePath(): NormalizedPath;
            getProjectReferences(): readonly ProjectReference[] | undefined;
            updateReferences(refs: readonly ProjectReference[] | undefined): void;
            getGlobalProjectErrors(): readonly Diagnostic[];
            getAllProjectErrors(): readonly Diagnostic[];
            setProjectErrors(projectErrors: Diagnostic[]): void;
            close(): void;
            getEffectiveTypeRoots(): string[];
        }
        class ExternalProject extends Project {
            externalProjectName: string;
            compileOnSaveEnabled: boolean;
            excludedFiles: readonly NormalizedPath[];
            updateGraph(): boolean;
            getExcludedFiles(): readonly NormalizedPath[];
        }
        function convertFormatOptions(protocolOptions: protocol.FormatCodeSettings): FormatCodeSettings;
        function convertCompilerOptions(protocolOptions: protocol.ExternalProjectCompilerOptions): CompilerOptions & protocol.CompileOnSaveMixin;
        function convertWatchOptions(protocolOptions: protocol.ExternalProjectCompilerOptions, currentDirectory?: string): WatchOptionsAndErrors | undefined;
        function convertTypeAcquisition(protocolOptions: protocol.InferredProjectCompilerOptions): TypeAcquisition | undefined;
        function tryConvertScriptKindName(scriptKindName: protocol.ScriptKindName | ScriptKind): ScriptKind;
        function convertScriptKindName(scriptKindName: protocol.ScriptKindName): ScriptKind;
        const maxProgramSizeForNonTsFiles: number;
        const ProjectsUpdatedInBackgroundEvent = "projectsUpdatedInBackground";
        interface ProjectsUpdatedInBackgroundEvent {
            eventName: typeof ProjectsUpdatedInBackgroundEvent;
            data: {
                openFiles: string[];
            };
        }
        const ProjectLoadingStartEvent = "projectLoadingStart";
        interface ProjectLoadingStartEvent {
            eventName: typeof ProjectLoadingStartEvent;
            data: {
                project: Project;
                reason: string;
            };
        }
        const ProjectLoadingFinishEvent = "projectLoadingFinish";
        interface ProjectLoadingFinishEvent {
            eventName: typeof ProjectLoadingFinishEvent;
            data: {
                project: Project;
            };
        }
        const LargeFileReferencedEvent = "largeFileReferenced";
        interface LargeFileReferencedEvent {
            eventName: typeof LargeFileReferencedEvent;
            data: {
                file: string;
                fileSize: number;
                maxFileSize: number;
            };
        }
        const ConfigFileDiagEvent = "configFileDiag";
        interface ConfigFileDiagEvent {
            eventName: typeof ConfigFileDiagEvent;
            data: {
                triggerFile: string;
                configFileName: string;
                diagnostics: readonly Diagnostic[];
            };
        }
        const ProjectLanguageServiceStateEvent = "projectLanguageServiceState";
        interface ProjectLanguageServiceStateEvent {
            eventName: typeof ProjectLanguageServiceStateEvent;
            data: {
                project: Project;
                languageServiceEnabled: boolean;
            };
        }
        const ProjectInfoTelemetryEvent = "projectInfo";
        interface ProjectInfoTelemetryEvent {
            readonly eventName: typeof ProjectInfoTelemetryEvent;
            readonly data: ProjectInfoTelemetryEventData;
        }
        const OpenFileInfoTelemetryEvent = "openFileInfo";
        interface OpenFileInfoTelemetryEvent {
            readonly eventName: typeof OpenFileInfoTelemetryEvent;
            readonly data: OpenFileInfoTelemetryEventData;
        }
        const CreateFileWatcherEvent: protocol.CreateFileWatcherEventName;
        interface CreateFileWatcherEvent {
            readonly eventName: protocol.CreateFileWatcherEventName;
            readonly data: protocol.CreateFileWatcherEventBody;
        }
        const CreateDirectoryWatcherEvent: protocol.CreateDirectoryWatcherEventName;
        interface CreateDirectoryWatcherEvent {
            readonly eventName: protocol.CreateDirectoryWatcherEventName;
            readonly data: protocol.CreateDirectoryWatcherEventBody;
        }
        const CloseFileWatcherEvent: protocol.CloseFileWatcherEventName;
        interface CloseFileWatcherEvent {
            readonly eventName: protocol.CloseFileWatcherEventName;
            readonly data: protocol.CloseFileWatcherEventBody;
        }
        interface ProjectInfoTelemetryEventData {
            readonly projectId: string;
            readonly fileStats: FileStats;
            readonly compilerOptions: CompilerOptions;
            readonly extends: boolean | undefined;
            readonly files: boolean | undefined;
            readonly include: boolean | undefined;
            readonly exclude: boolean | undefined;
            readonly compileOnSave: boolean;
            readonly typeAcquisition: ProjectInfoTypeAcquisitionData;
            readonly configFileName: "tsconfig.json" | "jsconfig.json" | "other";
            readonly projectType: "external" | "configured";
            readonly languageServiceEnabled: boolean;
            readonly version: string;
        }
        interface OpenFileInfoTelemetryEventData {
            readonly info: OpenFileInfo;
        }
        interface ProjectInfoTypeAcquisitionData {
            readonly enable: boolean | undefined;
            readonly include: boolean;
            readonly exclude: boolean;
        }
        interface FileStats {
            readonly js: number;
            readonly jsSize?: number;
            readonly jsx: number;
            readonly jsxSize?: number;
            readonly ts: number;
            readonly tsSize?: number;
            readonly tsx: number;
            readonly tsxSize?: number;
            readonly dts: number;
            readonly dtsSize?: number;
            readonly deferred: number;
            readonly deferredSize?: number;
        }
        interface OpenFileInfo {
            readonly checkJs: boolean;
        }
        type ProjectServiceEvent = LargeFileReferencedEvent | ProjectsUpdatedInBackgroundEvent | ProjectLoadingStartEvent | ProjectLoadingFinishEvent | ConfigFileDiagEvent | ProjectLanguageServiceStateEvent | ProjectInfoTelemetryEvent | OpenFileInfoTelemetryEvent | CreateFileWatcherEvent | CreateDirectoryWatcherEvent | CloseFileWatcherEvent;
        type ProjectServiceEventHandler = (event: ProjectServiceEvent) => void;
        interface SafeList {
            [name: string]: {
                match: RegExp;
                exclude?: (string | number)[][];
                types?: string[];
            };
        }
        interface TypesMapFile {
            typesMap: SafeList;
            simpleMap: {
                [libName: string]: string;
            };
        }
        interface HostConfiguration {
            formatCodeOptions: FormatCodeSettings;
            preferences: protocol.UserPreferences;
            hostInfo: string;
            extraFileExtensions?: FileExtensionInfo[];
            watchOptions?: WatchOptions;
        }
        interface OpenConfiguredProjectResult {
            configFileName?: NormalizedPath;
            configFileErrors?: readonly Diagnostic[];
        }
        const nullTypingsInstaller: ITypingsInstaller;
        interface ProjectServiceOptions {
            host: ServerHost;
            logger: Logger;
            cancellationToken: HostCancellationToken;
            useSingleInferredProject: boolean;
            useInferredProjectPerProjectRoot: boolean;
            typingsInstaller?: ITypingsInstaller;
            eventHandler?: ProjectServiceEventHandler;
            canUseWatchEvents?: boolean;
            suppressDiagnosticEvents?: boolean;
            throttleWaitMilliseconds?: number;
            globalPlugins?: readonly string[];
            pluginProbeLocations?: readonly string[];
            allowLocalPluginLoads?: boolean;
            typesMapLocation?: string;
            serverMode?: LanguageServiceMode;
            session: Session<unknown> | undefined;
            jsDocParsingMode?: JSDocParsingMode;
        }
        interface WatchOptionsAndErrors {
            watchOptions: WatchOptions;
            errors: Diagnostic[] | undefined;
        }
        class ProjectService {
            private readonly nodeModulesWatchers;
            private readonly filenameToScriptInfoVersion;
            private readonly allJsFilesForOpenFileTelemetry;
            private readonly externalProjectToConfiguredProjectMap;
            readonly externalProjects: ExternalProject[];
            readonly inferredProjects: InferredProject[];
            readonly configuredProjects: Map<string, ConfiguredProject>;
            readonly openFiles: Map<Path, NormalizedPath | undefined>;
            private readonly configFileForOpenFiles;
            private rootOfInferredProjects;
            private readonly openFilesWithNonRootedDiskPath;
            private compilerOptionsForInferredProjects;
            private compilerOptionsForInferredProjectsPerProjectRoot;
            private watchOptionsForInferredProjects;
            private watchOptionsForInferredProjectsPerProjectRoot;
            private typeAcquisitionForInferredProjects;
            private typeAcquisitionForInferredProjectsPerProjectRoot;
            private readonly projectToSizeMap;
            private readonly hostConfiguration;
            private safelist;
            private readonly legacySafelist;
            private pendingProjectUpdates;
            private pendingOpenFileProjectUpdates?;
            readonly currentDirectory: NormalizedPath;
            readonly toCanonicalFileName: (f: string) => string;
            readonly host: ServerHost;
            readonly logger: Logger;
            readonly cancellationToken: HostCancellationToken;
            readonly useSingleInferredProject: boolean;
            readonly useInferredProjectPerProjectRoot: boolean;
            readonly typingsInstaller: ITypingsInstaller;
            private readonly globalCacheLocationDirectoryPath;
            readonly throttleWaitMilliseconds?: number;
            private readonly suppressDiagnosticEvents?;
            readonly globalPlugins: readonly string[];
            readonly pluginProbeLocations: readonly string[];
            readonly allowLocalPluginLoads: boolean;
            readonly typesMapLocation: string | undefined;
            readonly serverMode: LanguageServiceMode;
            private readonly seenProjects;
            private readonly sharedExtendedConfigFileWatchers;
            private readonly extendedConfigCache;
            private packageJsonFilesMap;
            private incompleteCompletionsCache;
            private performanceEventHandler?;
            private pendingPluginEnablements?;
            private currentPluginEnablementPromise?;
            readonly jsDocParsingMode: JSDocParsingMode | undefined;
            constructor(opts: ProjectServiceOptions);
            toPath(fileName: string): Path;
            private loadTypesMap;
            updateTypingsForProject(response: SetTypings | InvalidateCachedTypings | PackageInstalledResponse): void;
            private delayUpdateProjectGraph;
            private delayUpdateProjectGraphs;
            setCompilerOptionsForInferredProjects(projectCompilerOptions: protocol.InferredProjectCompilerOptions, projectRootPath?: string): void;
            findProject(projectName: string): Project | undefined;
            getDefaultProjectForFile(fileName: NormalizedPath, ensureProject: boolean): Project | undefined;
            private tryGetDefaultProjectForEnsuringConfiguredProjectForFile;
            private doEnsureDefaultProjectForFile;
            getScriptInfoEnsuringProjectsUptoDate(uncheckedFileName: string): ScriptInfo | undefined;
            private ensureProjectStructuresUptoDate;
            getFormatCodeOptions(file: NormalizedPath): FormatCodeSettings;
            getPreferences(file: NormalizedPath): protocol.UserPreferences;
            getHostFormatCodeOptions(): FormatCodeSettings;
            getHostPreferences(): protocol.UserPreferences;
            private onSourceFileChanged;
            private handleSourceMapProjects;
            private delayUpdateSourceInfoProjects;
            private delayUpdateProjectsOfScriptInfoPath;
            private handleDeletedFile;
            private watchWildcardDirectory;
            private onWildCardDirectoryWatcherInvoke;
            private delayUpdateProjectsFromParsedConfigOnConfigFileChange;
            private onConfigFileChanged;
            private removeProject;
            private assignOrphanScriptInfosToInferredProject;
            private closeOpenFile;
            private deleteScriptInfo;
            private configFileExists;
            private createConfigFileWatcherForParsedConfig;
            private ensureConfigFileWatcherForProject;
            private forEachConfigFileLocation;
            private getConfigFileNameForFileFromCache;
            private setConfigFileNameForFileInCache;
            private printProjects;
            private getConfiguredProjectByCanonicalConfigFilePath;
            private findExternalProjectByProjectName;
            private getFilenameForExceededTotalSizeLimitForNonTsFiles;
            private createExternalProject;
            private addFilesToNonInferredProject;
            private loadConfiguredProject;
            private updateNonInferredProjectFiles;
            private updateRootAndOptionsOfNonInferredProject;
            private reloadFileNamesOfParsedConfig;
            private setProjectForReload;
            private clearSemanticCache;
            private getOrCreateInferredProjectForProjectRootPathIfEnabled;
            private getOrCreateSingleInferredProjectIfEnabled;
            private getOrCreateSingleInferredWithoutProjectRoot;
            private createInferredProject;
            getScriptInfo(uncheckedFileName: string): ScriptInfo | undefined;
            private watchClosedScriptInfo;
            private createNodeModulesWatcher;
            private watchClosedScriptInfoInNodeModules;
            private getModifiedTime;
            private refreshScriptInfo;
            private refreshScriptInfosInDirectory;
            private stopWatchingScriptInfo;
            private getOrCreateScriptInfoNotOpenedByClientForNormalizedPath;
            getOrCreateScriptInfoForNormalizedPath(fileName: NormalizedPath, openedByClient: boolean, fileContent?: string, scriptKind?: ScriptKind, hasMixedContent?: boolean, hostToQueryFileExistsOn?: {
                fileExists(path: string): boolean;
            }): ScriptInfo | undefined;
            private getOrCreateScriptInfoWorker;
            getScriptInfoForNormalizedPath(fileName: NormalizedPath): ScriptInfo | undefined;
            getScriptInfoForPath(fileName: Path): ScriptInfo | undefined;
            private addSourceInfoToSourceMap;
            private addMissingSourceMapFile;
            setHostConfiguration(args: protocol.ConfigureRequestArguments): void;
            private getWatchOptionsFromProjectWatchOptions;
            closeLog(): void;
            private sendSourceFileChange;
            reloadProjects(): void;
            private removeRootOfInferredProjectIfNowPartOfOtherProject;
            private ensureProjectForOpenFiles;
            openClientFile(fileName: string, fileContent?: string, scriptKind?: ScriptKind, projectRootPath?: string): OpenConfiguredProjectResult;
            private findExternalProjectContainingOpenScriptInfo;
            private getOrCreateOpenScriptInfo;
            private assignProjectToOpenedScriptInfo;
            private tryFindDefaultConfiguredProjectForOpenScriptInfo;
            private isMatchedByConfig;
            private tryFindDefaultConfiguredProjectForOpenScriptInfoOrClosedFileInfo;
            private tryFindDefaultConfiguredProjectAndLoadAncestorsForOpenScriptInfo;
            private ensureProjectChildren;
            private cleanupConfiguredProjects;
            private cleanupProjectsAndScriptInfos;
            private tryInvokeWildCardDirectories;
            openClientFileWithNormalizedPath(fileName: NormalizedPath, fileContent?: string, scriptKind?: ScriptKind, hasMixedContent?: boolean, projectRootPath?: NormalizedPath): OpenConfiguredProjectResult;
            private removeOrphanScriptInfos;
            private telemetryOnOpenFile;
            closeClientFile(uncheckedFileName: string): void;
            private collectChanges;
            closeExternalProject(uncheckedFileName: string): void;
            openExternalProjects(projects: protocol.ExternalProject[]): void;
            private static readonly filenameEscapeRegexp;
            private static escapeFilenameForRegex;
            resetSafeList(): void;
            applySafeList(proj: protocol.ExternalProject): NormalizedPath[];
            private applySafeListWorker;
            openExternalProject(proj: protocol.ExternalProject): void;
            hasDeferredExtension(): boolean;
            private endEnablePlugin;
            private enableRequestedPluginsAsync;
            private enableRequestedPluginsWorker;
            configurePlugin(args: protocol.ConfigurePluginRequestArguments): void;
            private watchPackageJsonFile;
            private onPackageJsonChange;
        }
        function formatMessage<T extends protocol.Message>(msg: T, logger: Logger, byteLength: (s: string, encoding: BufferEncoding) => number, newLine: string): string;
        interface ServerCancellationToken extends HostCancellationToken {
            setRequest(requestId: number): void;
            resetRequest(requestId: number): void;
        }
        const nullCancellationToken: ServerCancellationToken;
        type CommandNames = protocol.CommandTypes;
        const CommandNames: any;
        type Event = <T extends object>(body: T, eventName: string) => void;
        interface EventSender {
            event: Event;
        }
        interface SessionOptions {
            host: ServerHost;
            cancellationToken: ServerCancellationToken;
            useSingleInferredProject: boolean;
            useInferredProjectPerProjectRoot: boolean;
            typingsInstaller?: ITypingsInstaller;
            byteLength: (buf: string, encoding?: BufferEncoding) => number;
            hrtime: (start?: [
                number,
                number,
            ]) => [
                number,
                number,
            ];
            logger: Logger;
            canUseEvents: boolean;
            canUseWatchEvents?: boolean;
            eventHandler?: ProjectServiceEventHandler;
            suppressDiagnosticEvents?: boolean;
            serverMode?: LanguageServiceMode;
            throttleWaitMilliseconds?: number;
            noGetErrOnBackgroundUpdate?: boolean;
            globalPlugins?: readonly string[];
            pluginProbeLocations?: readonly string[];
            allowLocalPluginLoads?: boolean;
            typesMapLocation?: string;
        }
        class Session<TMessage = string> implements EventSender {
            private readonly gcTimer;
            protected projectService: ProjectService;
            private changeSeq;
            private performanceData;
            private currentRequestId;
            private errorCheck;
            protected host: ServerHost;
            private readonly cancellationToken;
            protected readonly typingsInstaller: ITypingsInstaller;
            protected byteLength: (buf: string, encoding?: BufferEncoding) => number;
            private hrtime;
            protected logger: Logger;
            protected canUseEvents: boolean;
            private suppressDiagnosticEvents?;
            private eventHandler;
            private readonly noGetErrOnBackgroundUpdate?;
            constructor(opts: SessionOptions);
            private sendRequestCompletedEvent;
            private addPerformanceData;
            private addDiagnosticsPerformanceData;
            private performanceEventHandler;
            private defaultEventHandler;
            private projectsUpdatedInBackgroundEvent;
            logError(err: Error, cmd: string): void;
            private logErrorWorker;
            send(msg: protocol.Message): void;
            protected writeMessage(msg: protocol.Message): void;
            event<T extends object>(body: T, eventName: string): void;
            private semanticCheck;
            private syntacticCheck;
            private suggestionCheck;
            private regionSemanticCheck;
            private sendDiagnosticsEvent;
            private updateErrorCheck;
            private cleanProjects;
            private cleanup;
            private getEncodedSyntacticClassifications;
            private getEncodedSemanticClassifications;
            private getProject;
            private getConfigFileAndProject;
            private getConfigFileDiagnostics;
            private convertToDiagnosticsWithLinePositionFromDiagnosticFile;
            private getCompilerOptionsDiagnostics;
            private convertToDiagnosticsWithLinePosition;
            private getDiagnosticsWorker;
            private getDefinition;
            private mapDefinitionInfoLocations;
            private getDefinitionAndBoundSpan;
            private findSourceDefinition;
            private getEmitOutput;
            private mapJSDocTagInfo;
            private mapDisplayParts;
            private mapSignatureHelpItems;
            private mapDefinitionInfo;
            private static mapToOriginalLocation;
            private toFileSpan;
            private toFileSpanWithContext;
            private getTypeDefinition;
            private mapImplementationLocations;
            private getImplementation;
            private getSyntacticDiagnosticsSync;
            private getSemanticDiagnosticsSync;
            private getSuggestionDiagnosticsSync;
            private getJsxClosingTag;
            private getLinkedEditingRange;
            private getDocumentHighlights;
            private provideInlayHints;
            private mapCode;
            private getCopilotRelatedInfo;
            private setCompilerOptionsForInferredProjects;
            private getProjectInfo;
            private getProjectInfoWorker;
            private getDefaultConfiguredProjectInfo;
            private getRenameInfo;
            private getProjects;
            private getDefaultProject;
            private getRenameLocations;
            private mapRenameInfo;
            private toSpanGroups;
            private getReferences;
            private getFileReferences;
            private openClientFile;
            private getPosition;
            private getPositionInFile;
            private getFileAndProject;
            private getFileAndLanguageServiceForSyntacticOperation;
            private getFileAndProjectWorker;
            private getOutliningSpans;
            private getTodoComments;
            private getDocCommentTemplate;
            private getSpanOfEnclosingComment;
            private getIndentation;
            private getBreakpointStatement;
            private getNameOrDottedNameSpan;
            private isValidBraceCompletion;
            private getQuickInfoWorker;
            private getFormattingEditsForRange;
            private getFormattingEditsForRangeFull;
            private getFormattingEditsForDocumentFull;
            private getFormattingEditsAfterKeystrokeFull;
            private getFormattingEditsAfterKeystroke;
            private getCompletions;
            private getCompletionEntryDetails;
            private getCompileOnSaveAffectedFileList;
            private emitFile;
            private getSignatureHelpItems;
            private toPendingErrorCheck;
            private getDiagnostics;
            private change;
            private reload;
            private saveToTmp;
            private closeClientFile;
            private mapLocationNavigationBarItems;
            private getNavigationBarItems;
            private toLocationNavigationTree;
            private getNavigationTree;
            private getNavigateToItems;
            private getFullNavigateToItems;
            private getSupportedCodeFixes;
            private isLocation;
            private extractPositionOrRange;
            private getRange;
            private getApplicableRefactors;
            private getEditsForRefactor;
            private getMoveToRefactoringFileSuggestions;
            private preparePasteEdits;
            private getPasteEdits;
            private organizeImports;
            private getEditsForFileRename;
            private getCodeFixes;
            private getCombinedCodeFix;
            private applyCodeActionCommand;
            private getStartAndEndPosition;
            private mapCodeAction;
            private mapCodeFixAction;
            private mapPasteEditsAction;
            private mapTextChangesToCodeEdits;
            private mapTextChangeToCodeEdit;
            private convertTextChangeToCodeEdit;
            private getBraceMatching;
            private getDiagnosticsForProject;
            private configurePlugin;
            private getSmartSelectionRange;
            private toggleLineComment;
            private toggleMultilineComment;
            private commentSelection;
            private uncommentSelection;
            private mapSelectionRange;
            private getScriptInfoFromProjectService;
            private toProtocolCallHierarchyItem;
            private toProtocolCallHierarchyIncomingCall;
            private toProtocolCallHierarchyOutgoingCall;
            private prepareCallHierarchy;
            private provideCallHierarchyIncomingCalls;
            private provideCallHierarchyOutgoingCalls;
            getCanonicalFileName(fileName: string): string;
            exit(): void;
            private notRequired;
            private requiredResponse;
            private handlers;
            addProtocolHandler(command: string, handler: (request: protocol.Request) => HandlerResponse): void;
            private setCurrentRequest;
            private resetCurrentRequest;
            executeWithRequestId<T>(requestId: number, f: () => T): T;
            executeCommand(request: protocol.Request): HandlerResponse;
            onMessage(message: TMessage): void;
            protected parseMessage(message: TMessage): protocol.Request;
            protected toStringMessage(message: TMessage): string;
            private getFormatOptions;
            private getPreferences;
            private getHostFormatOptions;
            private getHostPreferences;
        }
        interface HandlerResponse {
            response?: {};
            responseRequired?: boolean;
        }
    }
    namespace JsTyping {
        interface TypingResolutionHost {
            directoryExists(path: string): boolean;
            fileExists(fileName: string): boolean;
            readFile(path: string, encoding?: string): string | undefined;
            readDirectory(rootDir: string, extensions: readonly string[], excludes: readonly string[] | undefined, includes: readonly string[] | undefined, depth?: number): string[];
        }
    }
    const versionMajorMinor = "6.0";
    const version: string;
    interface MapLike<T> {
        [index: string]: T;
    }
    interface SortedReadonlyArray<T> extends ReadonlyArray<T> {
        " __sortedArrayBrand": any;
    }
    interface SortedArray<T> extends Array<T> {
        " __sortedArrayBrand": any;
    }
    type Path = string & {
        __pathBrand: any;
    };
    interface TextRange {
        pos: number;
        end: number;
    }
    interface ReadonlyTextRange {
        readonly pos: number;
        readonly end: number;
    }
    enum SyntaxKind {
        Unknown = 0,
        EndOfFileToken = 1,
        SingleLineCommentTrivia = 2,
        MultiLineCommentTrivia = 3,
        NewLineTrivia = 4,
        WhitespaceTrivia = 5,
        ShebangTrivia = 6,
        ConflictMarkerTrivia = 7,
        NonTextFileMarkerTrivia = 8,
        NumericLiteral = 9,
        BigIntLiteral = 10,
        StringLiteral = 11,
        JsxText = 12,
        JsxTextAllWhiteSpaces = 13,
        RegularExpressionLiteral = 14,
        NoSubstitutionTemplateLiteral = 15,
        TemplateHead = 16,
        TemplateMiddle = 17,
        TemplateTail = 18,
        OpenBraceToken = 19,
        CloseBraceToken = 20,
        OpenParenToken = 21,
        CloseParenToken = 22,
        OpenBracketToken = 23,
        CloseBracketToken = 24,
        DotToken = 25,
        DotDotDotToken = 26,
        SemicolonToken = 27,
        CommaToken = 28,
        QuestionDotToken = 29,
        LessThanToken = 30,
        LessThanSlashToken = 31,
        GreaterThanToken = 32,
        LessThanEqualsToken = 33,
        GreaterThanEqualsToken = 34,
        EqualsEqualsToken = 35,
        ExclamationEqualsToken = 36,
        EqualsEqualsEqualsToken = 37,
        ExclamationEqualsEqualsToken = 38,
        EqualsGreaterThanToken = 39,
        PlusToken = 40,
        MinusToken = 41,
        AsteriskToken = 42,
        AsteriskAsteriskToken = 43,
        SlashToken = 44,
        PercentToken = 45,
        PlusPlusToken = 46,
        MinusMinusToken = 47,
        LessThanLessThanToken = 48,
        GreaterThanGreaterThanToken = 49,
        GreaterThanGreaterThanGreaterThanToken = 50,
        AmpersandToken = 51,
        BarToken = 52,
        CaretToken = 53,
        ExclamationToken = 54,
        TildeToken = 55,
        AmpersandAmpersandToken = 56,
        BarBarToken = 57,
        QuestionToken = 58,
        ColonToken = 59,
        AtToken = 60,
        QuestionQuestionToken = 61,
        BacktickToken = 62,
        HashToken = 63,
        EqualsToken = 64,
        PlusEqualsToken = 65,
        MinusEqualsToken = 66,
        AsteriskEqualsToken = 67,
        AsteriskAsteriskEqualsToken = 68,
        SlashEqualsToken = 69,
        PercentEqualsToken = 70,
        LessThanLessThanEqualsToken = 71,
        GreaterThanGreaterThanEqualsToken = 72,
        GreaterThanGreaterThanGreaterThanEqualsToken = 73,
        AmpersandEqualsToken = 74,
        BarEqualsToken = 75,
        BarBarEqualsToken = 76,
        AmpersandAmpersandEqualsToken = 77,
        QuestionQuestionEqualsToken = 78,
        CaretEqualsToken = 79,
        Identifier = 80,
        PrivateIdentifier = 81,
        BreakKeyword = 83,
        CaseKeyword = 84,
        CatchKeyword = 85,
        ClassKeyword = 86,
        ConstKeyword = 87,
        ContinueKeyword = 88,
        DebuggerKeyword = 89,
        DefaultKeyword = 90,
        DeleteKeyword = 91,
        DoKeyword = 92,
        ElseKeyword = 93,
        EnumKeyword = 94,
        ExportKeyword = 95,
        ExtendsKeyword = 96,
        FalseKeyword = 97,
        FinallyKeyword = 98,
        ForKeyword = 99,
        FunctionKeyword = 100,
        IfKeyword = 101,
        ImportKeyword = 102,
        InKeyword = 103,
        InstanceOfKeyword = 104,
        NewKeyword = 105,
        NullKeyword = 106,
        ReturnKeyword = 107,
        SuperKeyword = 108,
        SwitchKeyword = 109,
        ThisKeyword = 110,
        ThrowKeyword = 111,
        TrueKeyword = 112,
        TryKeyword = 113,
        TypeOfKeyword = 114,
        VarKeyword = 115,
        VoidKeyword = 116,
        WhileKeyword = 117,
        WithKeyword = 118,
        ImplementsKeyword = 119,
        InterfaceKeyword = 120,
        LetKeyword = 121,
        PackageKeyword = 122,
        PrivateKeyword = 123,
        ProtectedKeyword = 124,
        PublicKeyword = 125,
        StaticKeyword = 126,
        YieldKeyword = 127,
        AbstractKeyword = 128,
        AccessorKeyword = 129,
        AsKeyword = 130,
        AssertsKeyword = 131,
        AssertKeyword = 132,
        AnyKeyword = 133,
        AsyncKeyword = 134,
        AwaitKeyword = 135,
        BooleanKeyword = 136,
        ConstructorKeyword = 137,
        DeclareKeyword = 138,
        GetKeyword = 139,
        InferKeyword = 140,
        IntrinsicKeyword = 141,
        IsKeyword = 142,
        KeyOfKeyword = 143,
        ModuleKeyword = 144,
        NamespaceKeyword = 145,
        NeverKeyword = 146,
        OutKeyword = 147,
        ReadonlyKeyword = 148,
        RequireKeyword = 149,
        NumberKeyword = 150,
        ObjectKeyword = 151,
        SatisfiesKeyword = 152,
        SetKeyword = 153,
        StringKeyword = 154,
        SymbolKeyword = 155,
        TypeKeyword = 156,
        UndefinedKeyword = 157,
        UniqueKeyword = 158,
        UnknownKeyword = 159,
        UsingKeyword = 160,
        FromKeyword = 161,
        GlobalKeyword = 162,
        BigIntKeyword = 163,
        OverrideKeyword = 164,
        OfKeyword = 165,
        DeferKeyword = 166,
        QualifiedName = 167,
        ComputedPropertyName = 168,
        TypeParameter = 169,
        Parameter = 170,
        Decorator = 171,
        PropertySignature = 172,
        PropertyDeclaration = 173,
        MethodSignature = 174,
        MethodDeclaration = 175,
        ClassStaticBlockDeclaration = 176,
        Constructor = 177,
        GetAccessor = 178,
        SetAccessor = 179,
        CallSignature = 180,
        ConstructSignature = 181,
        IndexSignature = 182,
        TypePredicate = 183,
        TypeReference = 184,
        FunctionType = 185,
        ConstructorType = 186,
        TypeQuery = 187,
        TypeLiteral = 188,
        ArrayType = 189,
        TupleType = 190,
        OptionalType = 191,
        RestType = 192,
        UnionType = 193,
        IntersectionType = 194,
        ConditionalType = 195,
        InferType = 196,
        ParenthesizedType = 197,
        ThisType = 198,
        TypeOperator = 199,
        IndexedAccessType = 200,
        MappedType = 201,
        LiteralType = 202,
        NamedTupleMember = 203,
        TemplateLiteralType = 204,
        TemplateLiteralTypeSpan = 205,
        ImportType = 206,
        ObjectBindingPattern = 207,
        ArrayBindingPattern = 208,
        BindingElement = 209,
        ArrayLiteralExpression = 210,
        ObjectLiteralExpression = 211,
        PropertyAccessExpression = 212,
        ElementAccessExpression = 213,
        CallExpression = 214,
        NewExpression = 215,
        TaggedTemplateExpression = 216,
        TypeAssertionExpression = 217,
        ParenthesizedExpression = 218,
        FunctionExpression = 219,
        ArrowFunction = 220,
        DeleteExpression = 221,
        TypeOfExpression = 222,
        VoidExpression = 223,
        AwaitExpression = 224,
        PrefixUnaryExpression = 225,
        PostfixUnaryExpression = 226,
        BinaryExpression = 227,
        ConditionalExpression = 228,
        TemplateExpression = 229,
        YieldExpression = 230,
        SpreadElement = 231,
        ClassExpression = 232,
        OmittedExpression = 233,
        ExpressionWithTypeArguments = 234,
        AsExpression = 235,
        NonNullExpression = 236,
        MetaProperty = 237,
        SyntheticExpression = 238,
        SatisfiesExpression = 239,
        TemplateSpan = 240,
        SemicolonClassElement = 241,
        Block = 242,
        EmptyStatement = 243,
        VariableStatement = 244,
        ExpressionStatement = 245,
        IfStatement = 246,
        DoStatement = 247,
        WhileStatement = 248,
        ForStatement = 249,
        ForInStatement = 250,
        ForOfStatement = 251,
        ContinueStatement = 252,
        BreakStatement = 253,
        ReturnStatement = 254,
        WithStatement = 255,
        SwitchStatement = 256,
        LabeledStatement = 257,
        ThrowStatement = 258,
        TryStatement = 259,
        DebuggerStatement = 260,
        VariableDeclaration = 261,
        VariableDeclarationList = 262,
        FunctionDeclaration = 263,
        ClassDeclaration = 264,
        InterfaceDeclaration = 265,
        TypeAliasDeclaration = 266,
        EnumDeclaration = 267,
        ModuleDeclaration = 268,
        ModuleBlock = 269,
        CaseBlock = 270,
        NamespaceExportDeclaration = 271,
        ImportEqualsDeclaration = 272,
        ImportDeclaration = 273,
        ImportClause = 274,
        NamespaceImport = 275,
        NamedImports = 276,
        ImportSpecifier = 277,
        ExportAssignment = 278,
        ExportDeclaration = 279,
        NamedExports = 280,
        NamespaceExport = 281,
        ExportSpecifier = 282,
        MissingDeclaration = 283,
        ExternalModuleReference = 284,
        JsxElement = 285,
        JsxSelfClosingElement = 286,
        JsxOpeningElement = 287,
        JsxClosingElement = 288,
        JsxFragment = 289,
        JsxOpeningFragment = 290,
        JsxClosingFragment = 291,
        JsxAttribute = 292,
        JsxAttributes = 293,
        JsxSpreadAttribute = 294,
        JsxExpression = 295,
        JsxNamespacedName = 296,
        CaseClause = 297,
        DefaultClause = 298,
        HeritageClause = 299,
        CatchClause = 300,
        ImportAttributes = 301,
        ImportAttribute = 302,
AssertClause = 301,
AssertEntry = 302,
ImportTypeAssertionContainer = 303,
        PropertyAssignment = 304,
        ShorthandPropertyAssignment = 305,
        SpreadAssignment = 306,
        EnumMember = 307,
        SourceFile = 308,
        Bundle = 309,
        JSDocTypeExpression = 310,
        JSDocNameReference = 311,
        JSDocMemberName = 312,
        JSDocAllType = 313,
        JSDocUnknownType = 314,
        JSDocNullableType = 315,
        JSDocNonNullableType = 316,
        JSDocOptionalType = 317,
        JSDocFunctionType = 318,
        JSDocVariadicType = 319,
        JSDocNamepathType = 320,
        JSDoc = 321,
        JSDocComment = 321,
        JSDocText = 322,
        JSDocTypeLiteral = 323,
        JSDocSignature = 324,
        JSDocLink = 325,
        JSDocLinkCode = 326,
        JSDocLinkPlain = 327,
        JSDocTag = 328,
        JSDocAugmentsTag = 329,
        JSDocImplementsTag = 330,
        JSDocAuthorTag = 331,
        JSDocDeprecatedTag = 332,
        JSDocClassTag = 333,
        JSDocPublicTag = 334,
        JSDocPrivateTag = 335,
        JSDocProtectedTag = 336,
        JSDocReadonlyTag = 337,
        JSDocOverrideTag = 338,
        JSDocCallbackTag = 339,
        JSDocOverloadTag = 340,
        JSDocEnumTag = 341,
        JSDocParameterTag = 342,
        JSDocReturnTag = 343,
        JSDocThisTag = 344,
        JSDocTypeTag = 345,
        JSDocTemplateTag = 346,
        JSDocTypedefTag = 347,
        JSDocSeeTag = 348,
        JSDocPropertyTag = 349,
        JSDocThrowsTag = 350,
        JSDocSatisfiesTag = 351,
        JSDocImportTag = 352,
        SyntaxList = 353,
        NotEmittedStatement = 354,
        NotEmittedTypeElement = 355,
        PartiallyEmittedExpression = 356,
        CommaListExpression = 357,
        SyntheticReferenceExpression = 358,
        Count = 359,
        FirstAssignment = 64,
        LastAssignment = 79,
        FirstCompoundAssignment = 65,
        LastCompoundAssignment = 79,
        FirstReservedWord = 83,
        LastReservedWord = 118,
        FirstKeyword = 83,
        LastKeyword = 166,
        FirstFutureReservedWord = 119,
        LastFutureReservedWord = 127,
        FirstTypeNode = 183,
        LastTypeNode = 206,
        FirstPunctuation = 19,
        LastPunctuation = 79,
        FirstToken = 0,
        LastToken = 166,
        FirstTriviaToken = 2,
        LastTriviaToken = 7,
        FirstLiteralToken = 9,
        LastLiteralToken = 15,
        FirstTemplateToken = 15,
        LastTemplateToken = 18,
        FirstBinaryOperator = 30,
        LastBinaryOperator = 79,
        FirstStatement = 244,
        LastStatement = 260,
        FirstNode = 167,
        FirstJSDocNode = 310,
        LastJSDocNode = 352,
        FirstJSDocTagNode = 328,
        LastJSDocTagNode = 352,
    }
    type TriviaSyntaxKind = SyntaxKind.SingleLineCommentTrivia | SyntaxKind.MultiLineCommentTrivia | SyntaxKind.NewLineTrivia | SyntaxKind.WhitespaceTrivia | SyntaxKind.ShebangTrivia | SyntaxKind.ConflictMarkerTrivia;
    type LiteralSyntaxKind = SyntaxKind.NumericLiteral | SyntaxKind.BigIntLiteral | SyntaxKind.StringLiteral | SyntaxKind.JsxText | SyntaxKind.JsxTextAllWhiteSpaces | SyntaxKind.RegularExpressionLiteral | SyntaxKind.NoSubstitutionTemplateLiteral;
    type PseudoLiteralSyntaxKind = SyntaxKind.TemplateHead | SyntaxKind.TemplateMiddle | SyntaxKind.TemplateTail;
    type PunctuationSyntaxKind =
        | SyntaxKind.OpenBraceToken
        | SyntaxKind.CloseBraceToken
        | SyntaxKind.OpenParenToken
        | SyntaxKind.CloseParenToken
        | SyntaxKind.OpenBracketToken
        | SyntaxKind.CloseBracketToken
        | SyntaxKind.DotToken
        | SyntaxKind.DotDotDotToken
        | SyntaxKind.SemicolonToken
        | SyntaxKind.CommaToken
        | SyntaxKind.QuestionDotToken
        | SyntaxKind.LessThanToken
        | SyntaxKind.LessThanSlashToken
        | SyntaxKind.GreaterThanToken
        | SyntaxKind.LessThanEqualsToken
        | SyntaxKind.GreaterThanEqualsToken
        | SyntaxKind.EqualsEqualsToken
        | SyntaxKind.ExclamationEqualsToken
        | SyntaxKind.EqualsEqualsEqualsToken
        | SyntaxKind.ExclamationEqualsEqualsToken
        | SyntaxKind.EqualsGreaterThanToken
        | SyntaxKind.PlusToken
        | SyntaxKind.MinusToken
        | SyntaxKind.AsteriskToken
        | SyntaxKind.AsteriskAsteriskToken
        | SyntaxKind.SlashToken
        | SyntaxKind.PercentToken
        | SyntaxKind.PlusPlusToken
        | SyntaxKind.MinusMinusToken
        | SyntaxKind.LessThanLessThanToken
        | SyntaxKind.GreaterThanGreaterThanToken
        | SyntaxKind.GreaterThanGreaterThanGreaterThanToken
        | SyntaxKind.AmpersandToken
        | SyntaxKind.BarToken
        | SyntaxKind.CaretToken
        | SyntaxKind.ExclamationToken
        | SyntaxKind.TildeToken
        | SyntaxKind.AmpersandAmpersandToken
        | SyntaxKind.AmpersandAmpersandEqualsToken
        | SyntaxKind.BarBarToken
        | SyntaxKind.BarBarEqualsToken
        | SyntaxKind.QuestionQuestionToken
        | SyntaxKind.QuestionQuestionEqualsToken
        | SyntaxKind.QuestionToken
        | SyntaxKind.ColonToken
        | SyntaxKind.AtToken
        | SyntaxKind.BacktickToken
        | SyntaxKind.HashToken
        | SyntaxKind.EqualsToken
        | SyntaxKind.PlusEqualsToken
        | SyntaxKind.MinusEqualsToken
        | SyntaxKind.AsteriskEqualsToken
        | SyntaxKind.AsteriskAsteriskEqualsToken
        | SyntaxKind.SlashEqualsToken
        | SyntaxKind.PercentEqualsToken
        | SyntaxKind.LessThanLessThanEqualsToken
        | SyntaxKind.GreaterThanGreaterThanEqualsToken
        | SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken
        | SyntaxKind.AmpersandEqualsToken
        | SyntaxKind.BarEqualsToken
        | SyntaxKind.CaretEqualsToken;
    type KeywordSyntaxKind =
        | SyntaxKind.AbstractKeyword
        | SyntaxKind.AccessorKeyword
        | SyntaxKind.AnyKeyword
        | SyntaxKind.AsKeyword
        | SyntaxKind.AssertsKeyword
        | SyntaxKind.AssertKeyword
        | SyntaxKind.AsyncKeyword
        | SyntaxKind.AwaitKeyword
        | SyntaxKind.BigIntKeyword
        | SyntaxKind.BooleanKeyword
        | SyntaxKind.BreakKeyword
        | SyntaxKind.CaseKeyword
        | SyntaxKind.CatchKeyword
        | SyntaxKind.ClassKeyword
        | SyntaxKind.ConstKeyword
        | SyntaxKind.ConstructorKeyword
        | SyntaxKind.ContinueKeyword
        | SyntaxKind.DebuggerKeyword
        | SyntaxKind.DeclareKeyword
        | SyntaxKind.DefaultKeyword
        | SyntaxKind.DeferKeyword
        | SyntaxKind.DeleteKeyword
        | SyntaxKind.DoKeyword
        | SyntaxKind.ElseKeyword
        | SyntaxKind.EnumKeyword
        | SyntaxKind.ExportKeyword
        | SyntaxKind.ExtendsKeyword
        | SyntaxKind.FalseKeyword
        | SyntaxKind.FinallyKeyword
        | SyntaxKind.ForKeyword
        | SyntaxKind.FromKeyword
        | SyntaxKind.FunctionKeyword
        | SyntaxKind.GetKeyword
        | SyntaxKind.GlobalKeyword
        | SyntaxKind.IfKeyword
        | SyntaxKind.ImplementsKeyword
        | SyntaxKind.ImportKeyword
        | SyntaxKind.InferKeyword
        | SyntaxKind.InKeyword
        | SyntaxKind.InstanceOfKeyword
        | SyntaxKind.InterfaceKeyword
        | SyntaxKind.IntrinsicKeyword
        | SyntaxKind.IsKeyword
        | SyntaxKind.KeyOfKeyword
        | SyntaxKind.LetKeyword
        | SyntaxKind.ModuleKeyword
        | SyntaxKind.NamespaceKeyword
        | SyntaxKind.NeverKeyword
        | SyntaxKind.NewKeyword
        | SyntaxKind.NullKeyword
        | SyntaxKind.NumberKeyword
        | SyntaxKind.ObjectKeyword
        | SyntaxKind.OfKeyword
        | SyntaxKind.PackageKeyword
        | SyntaxKind.PrivateKeyword
        | SyntaxKind.ProtectedKeyword
        | SyntaxKind.PublicKeyword
        | SyntaxKind.ReadonlyKeyword
        | SyntaxKind.OutKeyword
        | SyntaxKind.OverrideKeyword
        | SyntaxKind.RequireKeyword
        | SyntaxKind.ReturnKeyword
        | SyntaxKind.SatisfiesKeyword
        | SyntaxKind.SetKeyword
        | SyntaxKind.StaticKeyword
        | SyntaxKind.StringKeyword
        | SyntaxKind.SuperKeyword
        | SyntaxKind.SwitchKeyword
        | SyntaxKind.SymbolKeyword
        | SyntaxKind.ThisKeyword
        | SyntaxKind.ThrowKeyword
        | SyntaxKind.TrueKeyword
        | SyntaxKind.TryKeyword
        | SyntaxKind.TypeKeyword
        | SyntaxKind.TypeOfKeyword
        | SyntaxKind.UndefinedKeyword
        | SyntaxKind.UniqueKeyword
        | SyntaxKind.UnknownKeyword
        | SyntaxKind.UsingKeyword
        | SyntaxKind.VarKeyword
        | SyntaxKind.VoidKeyword
        | SyntaxKind.WhileKeyword
        | SyntaxKind.WithKeyword
        | SyntaxKind.YieldKeyword;
    type ModifierSyntaxKind = SyntaxKind.AbstractKeyword | SyntaxKind.AccessorKeyword | SyntaxKind.AsyncKeyword | SyntaxKind.ConstKeyword | SyntaxKind.DeclareKeyword | SyntaxKind.DefaultKeyword | SyntaxKind.ExportKeyword | SyntaxKind.InKeyword | SyntaxKind.PrivateKeyword | SyntaxKind.ProtectedKeyword | SyntaxKind.PublicKeyword | SyntaxKind.ReadonlyKeyword | SyntaxKind.OutKeyword | SyntaxKind.OverrideKeyword | SyntaxKind.StaticKeyword;
    type KeywordTypeSyntaxKind = SyntaxKind.AnyKeyword | SyntaxKind.BigIntKeyword | SyntaxKind.BooleanKeyword | SyntaxKind.IntrinsicKeyword | SyntaxKind.NeverKeyword | SyntaxKind.NumberKeyword | SyntaxKind.ObjectKeyword | SyntaxKind.StringKeyword | SyntaxKind.SymbolKeyword | SyntaxKind.UndefinedKeyword | SyntaxKind.UnknownKeyword | SyntaxKind.VoidKeyword;
    type TokenSyntaxKind = SyntaxKind.Unknown | SyntaxKind.EndOfFileToken | TriviaSyntaxKind | LiteralSyntaxKind | PseudoLiteralSyntaxKind | PunctuationSyntaxKind | SyntaxKind.Identifier | KeywordSyntaxKind;
    type JsxTokenSyntaxKind = SyntaxKind.LessThanSlashToken | SyntaxKind.EndOfFileToken | SyntaxKind.ConflictMarkerTrivia | SyntaxKind.JsxText | SyntaxKind.JsxTextAllWhiteSpaces | SyntaxKind.OpenBraceToken | SyntaxKind.LessThanToken;
    type JSDocSyntaxKind = SyntaxKind.EndOfFileToken | SyntaxKind.WhitespaceTrivia | SyntaxKind.AtToken | SyntaxKind.NewLineTrivia | SyntaxKind.AsteriskToken | SyntaxKind.OpenBraceToken | SyntaxKind.CloseBraceToken | SyntaxKind.LessThanToken | SyntaxKind.GreaterThanToken | SyntaxKind.OpenBracketToken | SyntaxKind.CloseBracketToken | SyntaxKind.OpenParenToken | SyntaxKind.CloseParenToken | SyntaxKind.EqualsToken | SyntaxKind.CommaToken | SyntaxKind.DotToken | SyntaxKind.Identifier | SyntaxKind.BacktickToken | SyntaxKind.HashToken | SyntaxKind.Unknown | KeywordSyntaxKind;
    enum NodeFlags {
        None = 0,
        Let = 1,
        Const = 2,
        Using = 4,
        AwaitUsing = 6,
        NestedNamespace = 8,
        Synthesized = 16,
        Namespace = 32,
        OptionalChain = 64,
        ExportContext = 128,
        ContainsThis = 256,
        HasImplicitReturn = 512,
        HasExplicitReturn = 1024,
        GlobalAugmentation = 2048,
        HasAsyncFunctions = 4096,
        DisallowInContext = 8192,
        YieldContext = 16384,
        DecoratorContext = 32768,
        AwaitContext = 65536,
        DisallowConditionalTypesContext = 131072,
        ThisNodeHasError = 262144,
        JavaScriptFile = 524288,
        ThisNodeOrAnySubNodesHasError = 1048576,
        HasAggregatedChildData = 2097152,
        JSDoc = 16777216,
        JsonFile = 134217728,
        BlockScoped = 7,
        Constant = 6,
        ReachabilityCheckFlags = 1536,
        ReachabilityAndEmitFlags = 5632,
        ContextFlags = 101441536,
        TypeExcludesFlags = 81920,
    }
    enum ModifierFlags {
        None = 0,
        Public = 1,
        Private = 2,
        Protected = 4,
        Readonly = 8,
        Override = 16,
        Export = 32,
        Abstract = 64,
        Ambient = 128,
        Static = 256,
        Accessor = 512,
        Async = 1024,
        Default = 2048,
        Const = 4096,
        In = 8192,
        Out = 16384,
        Decorator = 32768,
        Deprecated = 65536,
        HasComputedJSDocModifiers = 268435456,
        HasComputedFlags = 536870912,
        AccessibilityModifier = 7,
        ParameterPropertyModifier = 31,
        NonPublicAccessibilityModifier = 6,
        TypeScriptModifier = 28895,
        ExportDefault = 2080,
        All = 131071,
        Modifier = 98303,
    }
    enum JsxFlags {
        None = 0,
        IntrinsicNamedElement = 1,
        IntrinsicIndexedElement = 2,
        IntrinsicElement = 3,
    }
    interface Node extends ReadonlyTextRange {
        readonly kind: SyntaxKind;
        readonly flags: NodeFlags;
        readonly parent: Node;
    }
    interface Node {
        getSourceFile(): SourceFile;
        getChildCount(sourceFile?: SourceFile): number;
        getChildAt(index: number, sourceFile?: SourceFile): Node;
        getChildren(sourceFile?: SourceFile): readonly Node[];
        getStart(sourceFile?: SourceFile, includeJsDocComment?: boolean): number;
        getFullStart(): number;
        getEnd(): number;
        getWidth(sourceFile?: SourceFileLike): number;
        getFullWidth(): number;
        getLeadingTriviaWidth(sourceFile?: SourceFile): number;
        getFullText(sourceFile?: SourceFile): string;
        getText(sourceFile?: SourceFile): string;
        getFirstToken(sourceFile?: SourceFile): Node | undefined;
        getLastToken(sourceFile?: SourceFile): Node | undefined;
        forEachChild<T>(cbNode: (node: Node) => T | undefined, cbNodeArray?: (nodes: NodeArray<Node>) => T | undefined): T | undefined;
    }
    interface JSDocContainer extends Node {
        _jsdocContainerBrand: any;
    }
    interface LocalsContainer extends Node {
        _localsContainerBrand: any;
    }
    interface FlowContainer extends Node {
        _flowContainerBrand: any;
    }
    type HasJSDoc =
        | AccessorDeclaration
        | ArrowFunction
        | BinaryExpression
        | Block
        | BreakStatement
        | CallSignatureDeclaration
        | CaseClause
        | ClassLikeDeclaration
        | ClassStaticBlockDeclaration
        | ConstructorDeclaration
        | ConstructorTypeNode
        | ConstructSignatureDeclaration
        | ContinueStatement
        | DebuggerStatement
        | DoStatement
        | ElementAccessExpression
        | EmptyStatement
        | EndOfFileToken
        | EnumDeclaration
        | EnumMember
        | ExportAssignment
        | ExportDeclaration
        | ExportSpecifier
        | ExpressionStatement
        | ForInStatement
        | ForOfStatement
        | ForStatement
        | FunctionDeclaration
        | FunctionExpression
        | FunctionTypeNode
        | Identifier
        | IfStatement
        | ImportDeclaration
        | ImportEqualsDeclaration
        | IndexSignatureDeclaration
        | InterfaceDeclaration
        | JSDocFunctionType
        | JSDocSignature
        | LabeledStatement
        | MethodDeclaration
        | MethodSignature
        | ModuleDeclaration
        | NamedTupleMember
        | NamespaceExportDeclaration
        | ObjectLiteralExpression
        | ParameterDeclaration
        | ParenthesizedExpression
        | PropertyAccessExpression
        | PropertyAssignment
        | PropertyDeclaration
        | PropertySignature
        | ReturnStatement
        | SemicolonClassElement
        | ShorthandPropertyAssignment
        | SpreadAssignment
        | SwitchStatement
        | ThrowStatement
        | TryStatement
        | TypeAliasDeclaration
        | TypeParameterDeclaration
        | VariableDeclaration
        | VariableStatement
        | WhileStatement
        | WithStatement;
    type HasType = SignatureDeclaration | VariableDeclaration | ParameterDeclaration | PropertySignature | PropertyDeclaration | TypePredicateNode | ParenthesizedTypeNode | TypeOperatorNode | MappedTypeNode | AssertionExpression | TypeAliasDeclaration | JSDocTypeExpression | JSDocNonNullableType | JSDocNullableType | JSDocOptionalType | JSDocVariadicType;
    type HasTypeArguments = CallExpression | NewExpression | TaggedTemplateExpression | JsxOpeningElement | JsxSelfClosingElement;
    type HasInitializer = HasExpressionInitializer | ForStatement | ForInStatement | ForOfStatement | JsxAttribute;
    type HasExpressionInitializer = VariableDeclaration | ParameterDeclaration | BindingElement | PropertyDeclaration | PropertyAssignment | EnumMember;
    type HasDecorators = ParameterDeclaration | PropertyDeclaration | MethodDeclaration | GetAccessorDeclaration | SetAccessorDeclaration | ClassExpression | ClassDeclaration;
    type HasModifiers = TypeParameterDeclaration | ParameterDeclaration | ConstructorTypeNode | PropertySignature | PropertyDeclaration | MethodSignature | MethodDeclaration | ConstructorDeclaration | GetAccessorDeclaration | SetAccessorDeclaration | IndexSignatureDeclaration | FunctionExpression | ArrowFunction | ClassExpression | VariableStatement | FunctionDeclaration | ClassDeclaration | InterfaceDeclaration | TypeAliasDeclaration | EnumDeclaration | ModuleDeclaration | ImportEqualsDeclaration | ImportDeclaration | ExportAssignment | ExportDeclaration;
    interface NodeArray<T extends Node> extends ReadonlyArray<T>, ReadonlyTextRange {
        readonly hasTrailingComma: boolean;
    }
    interface Token<TKind extends SyntaxKind> extends Node {
        readonly kind: TKind;
    }
    type EndOfFileToken = Token<SyntaxKind.EndOfFileToken> & JSDocContainer;
    interface PunctuationToken<TKind extends PunctuationSyntaxKind> extends Token<TKind> {
    }
    type DotToken = PunctuationToken<SyntaxKind.DotToken>;
    type DotDotDotToken = PunctuationToken<SyntaxKind.DotDotDotToken>;
    type QuestionToken = PunctuationToken<SyntaxKind.QuestionToken>;
    type ExclamationToken = PunctuationToken<SyntaxKind.ExclamationToken>;
    type ColonToken = PunctuationToken<SyntaxKind.ColonToken>;
    type EqualsToken = PunctuationToken<SyntaxKind.EqualsToken>;
    type AmpersandAmpersandEqualsToken = PunctuationToken<SyntaxKind.AmpersandAmpersandEqualsToken>;
    type BarBarEqualsToken = PunctuationToken<SyntaxKind.BarBarEqualsToken>;
    type QuestionQuestionEqualsToken = PunctuationToken<SyntaxKind.QuestionQuestionEqualsToken>;
    type AsteriskToken = PunctuationToken<SyntaxKind.AsteriskToken>;
    type EqualsGreaterThanToken = PunctuationToken<SyntaxKind.EqualsGreaterThanToken>;
    type PlusToken = PunctuationToken<SyntaxKind.PlusToken>;
    type MinusToken = PunctuationToken<SyntaxKind.MinusToken>;
    type QuestionDotToken = PunctuationToken<SyntaxKind.QuestionDotToken>;
    interface KeywordToken<TKind extends KeywordSyntaxKind> extends Token<TKind> {
    }
    type AssertsKeyword = KeywordToken<SyntaxKind.AssertsKeyword>;
    type AssertKeyword = KeywordToken<SyntaxKind.AssertKeyword>;
    type AwaitKeyword = KeywordToken<SyntaxKind.AwaitKeyword>;
    type CaseKeyword = KeywordToken<SyntaxKind.CaseKeyword>;
    interface ModifierToken<TKind extends ModifierSyntaxKind> extends KeywordToken<TKind> {
    }
    type AbstractKeyword = ModifierToken<SyntaxKind.AbstractKeyword>;
    type AccessorKeyword = ModifierToken<SyntaxKind.AccessorKeyword>;
    type AsyncKeyword = ModifierToken<SyntaxKind.AsyncKeyword>;
    type ConstKeyword = ModifierToken<SyntaxKind.ConstKeyword>;
    type DeclareKeyword = ModifierToken<SyntaxKind.DeclareKeyword>;
    type DefaultKeyword = ModifierToken<SyntaxKind.DefaultKeyword>;
    type ExportKeyword = ModifierToken<SyntaxKind.ExportKeyword>;
    type InKeyword = ModifierToken<SyntaxKind.InKeyword>;
    type PrivateKeyword = ModifierToken<SyntaxKind.PrivateKeyword>;
    type ProtectedKeyword = ModifierToken<SyntaxKind.ProtectedKeyword>;
    type PublicKeyword = ModifierToken<SyntaxKind.PublicKeyword>;
    type ReadonlyKeyword = ModifierToken<SyntaxKind.ReadonlyKeyword>;
    type OutKeyword = ModifierToken<SyntaxKind.OutKeyword>;
    type OverrideKeyword = ModifierToken<SyntaxKind.OverrideKeyword>;
    type StaticKeyword = ModifierToken<SyntaxKind.StaticKeyword>;
    type Modifier = AbstractKeyword | AccessorKeyword | AsyncKeyword | ConstKeyword | DeclareKeyword | DefaultKeyword | ExportKeyword | InKeyword | PrivateKeyword | ProtectedKeyword | PublicKeyword | OutKeyword | OverrideKeyword | ReadonlyKeyword | StaticKeyword;
    type ModifierLike = Modifier | Decorator;
    type AccessibilityModifier = PublicKeyword | PrivateKeyword | ProtectedKeyword;
    type ParameterPropertyModifier = AccessibilityModifier | ReadonlyKeyword;
    type ClassMemberModifier = AccessibilityModifier | ReadonlyKeyword | StaticKeyword | AccessorKeyword;
    type ModifiersArray = NodeArray<Modifier>;
    enum GeneratedIdentifierFlags {
        None = 0,
        ReservedInNestedScopes = 8,
        Optimistic = 16,
        FileLevel = 32,
        AllowNameSubstitution = 64,
    }
    interface Identifier extends PrimaryExpression, Declaration, JSDocContainer, FlowContainer {
        readonly kind: SyntaxKind.Identifier;
        readonly escapedText: __String;
    }
    interface Identifier {
        readonly text: string;
    }
    interface TransientIdentifier extends Identifier {
        resolvedSymbol: Symbol;
    }
    interface QualifiedName extends Node, FlowContainer {
        readonly kind: SyntaxKind.QualifiedName;
        readonly left: EntityName;
        readonly right: Identifier;
    }
    type EntityName = Identifier | QualifiedName;
    type PropertyName = Identifier | StringLiteral | NoSubstitutionTemplateLiteral | NumericLiteral | ComputedPropertyName | PrivateIdentifier | BigIntLiteral;
    type MemberName = Identifier | PrivateIdentifier;
    type DeclarationName = PropertyName | JsxAttributeName | StringLiteralLike | ElementAccessExpression | BindingPattern | EntityNameExpression;
    interface Declaration extends Node {
        _declarationBrand: any;
    }
    interface NamedDeclaration extends Declaration {
        readonly name?: DeclarationName;
    }
    interface DeclarationStatement extends NamedDeclaration, Statement {
        readonly name?: Identifier | StringLiteral | NumericLiteral;
    }
    interface ComputedPropertyName extends Node {
        readonly kind: SyntaxKind.ComputedPropertyName;
        readonly parent: Declaration;
        readonly expression: Expression;
    }
    interface PrivateIdentifier extends PrimaryExpression {
        readonly kind: SyntaxKind.PrivateIdentifier;
        readonly escapedText: __String;
    }
    interface PrivateIdentifier {
        readonly text: string;
    }
    interface Decorator extends Node {
        readonly kind: SyntaxKind.Decorator;
        readonly parent: NamedDeclaration;
        readonly expression: LeftHandSideExpression;
    }
    interface TypeParameterDeclaration extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.TypeParameter;
        readonly parent: DeclarationWithTypeParameterChildren | InferTypeNode;
        readonly modifiers?: NodeArray<Modifier>;
        readonly name: Identifier;
        readonly constraint?: TypeNode;
        readonly default?: TypeNode;
        expression?: Expression;
    }
    interface SignatureDeclarationBase extends NamedDeclaration, JSDocContainer {
        readonly kind: SignatureDeclaration["kind"];
        readonly name?: PropertyName;
        readonly typeParameters?: NodeArray<TypeParameterDeclaration> | undefined;
        readonly parameters: NodeArray<ParameterDeclaration>;
        readonly type?: TypeNode | undefined;
    }
    type SignatureDeclaration = CallSignatureDeclaration | ConstructSignatureDeclaration | MethodSignature | IndexSignatureDeclaration | FunctionTypeNode | ConstructorTypeNode | JSDocFunctionType | FunctionDeclaration | MethodDeclaration | ConstructorDeclaration | AccessorDeclaration | FunctionExpression | ArrowFunction;
    interface CallSignatureDeclaration extends SignatureDeclarationBase, TypeElement, LocalsContainer {
        readonly kind: SyntaxKind.CallSignature;
    }
    interface ConstructSignatureDeclaration extends SignatureDeclarationBase, TypeElement, LocalsContainer {
        readonly kind: SyntaxKind.ConstructSignature;
    }
    type BindingName = Identifier | BindingPattern;
    interface VariableDeclaration extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.VariableDeclaration;
        readonly parent: VariableDeclarationList | CatchClause;
        readonly name: BindingName;
        readonly exclamationToken?: ExclamationToken;
        readonly type?: TypeNode;
        readonly initializer?: Expression;
    }
    interface VariableDeclarationList extends Node {
        readonly kind: SyntaxKind.VariableDeclarationList;
        readonly parent: VariableStatement | ForStatement | ForOfStatement | ForInStatement;
        readonly declarations: NodeArray<VariableDeclaration>;
    }
    interface ParameterDeclaration extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.Parameter;
        readonly parent: SignatureDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly dotDotDotToken?: DotDotDotToken;
        readonly name: BindingName;
        readonly questionToken?: QuestionToken;
        readonly type?: TypeNode;
        readonly initializer?: Expression;
    }
    interface BindingElement extends NamedDeclaration, FlowContainer {
        readonly kind: SyntaxKind.BindingElement;
        readonly parent: BindingPattern;
        readonly propertyName?: PropertyName;
        readonly dotDotDotToken?: DotDotDotToken;
        readonly name: BindingName;
        readonly initializer?: Expression;
    }
    interface PropertySignature extends TypeElement, JSDocContainer {
        readonly kind: SyntaxKind.PropertySignature;
        readonly parent: TypeLiteralNode | InterfaceDeclaration;
        readonly modifiers?: NodeArray<Modifier>;
        readonly name: PropertyName;
        readonly questionToken?: QuestionToken;
        readonly type?: TypeNode;
    }
    interface PropertyDeclaration extends ClassElement, JSDocContainer {
        readonly kind: SyntaxKind.PropertyDeclaration;
        readonly parent: ClassLikeDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: PropertyName;
        readonly questionToken?: QuestionToken;
        readonly exclamationToken?: ExclamationToken;
        readonly type?: TypeNode;
        readonly initializer?: Expression;
    }
    interface AutoAccessorPropertyDeclaration extends PropertyDeclaration {
        _autoAccessorBrand: any;
    }
    interface ObjectLiteralElement extends NamedDeclaration {
        _objectLiteralBrand: any;
        readonly name?: PropertyName;
    }
    type ObjectLiteralElementLike = PropertyAssignment | ShorthandPropertyAssignment | SpreadAssignment | MethodDeclaration | AccessorDeclaration;
    interface PropertyAssignment extends ObjectLiteralElement, JSDocContainer {
        readonly kind: SyntaxKind.PropertyAssignment;
        readonly parent: ObjectLiteralExpression;
        readonly name: PropertyName;
        readonly initializer: Expression;
    }
    interface ShorthandPropertyAssignment extends ObjectLiteralElement, JSDocContainer {
        readonly kind: SyntaxKind.ShorthandPropertyAssignment;
        readonly parent: ObjectLiteralExpression;
        readonly name: Identifier;
        readonly equalsToken?: EqualsToken;
        readonly objectAssignmentInitializer?: Expression;
    }
    interface SpreadAssignment extends ObjectLiteralElement, JSDocContainer {
        readonly kind: SyntaxKind.SpreadAssignment;
        readonly parent: ObjectLiteralExpression;
        readonly expression: Expression;
    }
    type VariableLikeDeclaration = VariableDeclaration | ParameterDeclaration | BindingElement | PropertyDeclaration | PropertyAssignment | PropertySignature | JsxAttribute | ShorthandPropertyAssignment | EnumMember | JSDocPropertyTag | JSDocParameterTag;
    interface ObjectBindingPattern extends Node {
        readonly kind: SyntaxKind.ObjectBindingPattern;
        readonly parent: VariableDeclaration | ParameterDeclaration | BindingElement;
        readonly elements: NodeArray<BindingElement>;
    }
    interface ArrayBindingPattern extends Node {
        readonly kind: SyntaxKind.ArrayBindingPattern;
        readonly parent: VariableDeclaration | ParameterDeclaration | BindingElement;
        readonly elements: NodeArray<ArrayBindingElement>;
    }
    type BindingPattern = ObjectBindingPattern | ArrayBindingPattern;
    type ArrayBindingElement = BindingElement | OmittedExpression;
    interface FunctionLikeDeclarationBase extends SignatureDeclarationBase {
        _functionLikeDeclarationBrand: any;
        readonly asteriskToken?: AsteriskToken | undefined;
        readonly questionToken?: QuestionToken | undefined;
        readonly exclamationToken?: ExclamationToken | undefined;
        readonly body?: Block | Expression | undefined;
    }
    type FunctionLikeDeclaration = FunctionDeclaration | MethodDeclaration | GetAccessorDeclaration | SetAccessorDeclaration | ConstructorDeclaration | FunctionExpression | ArrowFunction;
    type FunctionLike = SignatureDeclaration;
    interface FunctionDeclaration extends FunctionLikeDeclarationBase, DeclarationStatement, LocalsContainer {
        readonly kind: SyntaxKind.FunctionDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name?: Identifier;
        readonly body?: FunctionBody;
    }
    interface MethodSignature extends SignatureDeclarationBase, TypeElement, LocalsContainer {
        readonly kind: SyntaxKind.MethodSignature;
        readonly parent: TypeLiteralNode | InterfaceDeclaration;
        readonly modifiers?: NodeArray<Modifier>;
        readonly name: PropertyName;
    }
    interface MethodDeclaration extends FunctionLikeDeclarationBase, ClassElement, ObjectLiteralElement, JSDocContainer, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.MethodDeclaration;
        readonly parent: ClassLikeDeclaration | ObjectLiteralExpression;
        readonly modifiers?: NodeArray<ModifierLike> | undefined;
        readonly name: PropertyName;
        readonly body?: FunctionBody | undefined;
    }
    interface ConstructorDeclaration extends FunctionLikeDeclarationBase, ClassElement, JSDocContainer, LocalsContainer {
        readonly kind: SyntaxKind.Constructor;
        readonly parent: ClassLikeDeclaration;
        readonly modifiers?: NodeArray<ModifierLike> | undefined;
        readonly body?: FunctionBody | undefined;
    }
    interface SemicolonClassElement extends ClassElement, JSDocContainer {
        readonly kind: SyntaxKind.SemicolonClassElement;
        readonly parent: ClassLikeDeclaration;
    }
    interface GetAccessorDeclaration extends FunctionLikeDeclarationBase, ClassElement, TypeElement, ObjectLiteralElement, JSDocContainer, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.GetAccessor;
        readonly parent: ClassLikeDeclaration | ObjectLiteralExpression | TypeLiteralNode | InterfaceDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: PropertyName;
        readonly body?: FunctionBody;
    }
    interface SetAccessorDeclaration extends FunctionLikeDeclarationBase, ClassElement, TypeElement, ObjectLiteralElement, JSDocContainer, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.SetAccessor;
        readonly parent: ClassLikeDeclaration | ObjectLiteralExpression | TypeLiteralNode | InterfaceDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: PropertyName;
        readonly body?: FunctionBody;
    }
    type AccessorDeclaration = GetAccessorDeclaration | SetAccessorDeclaration;
    interface IndexSignatureDeclaration extends SignatureDeclarationBase, ClassElement, TypeElement, LocalsContainer {
        readonly kind: SyntaxKind.IndexSignature;
        readonly parent: ObjectTypeDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly type: TypeNode;
    }
    interface ClassStaticBlockDeclaration extends ClassElement, JSDocContainer, LocalsContainer {
        readonly kind: SyntaxKind.ClassStaticBlockDeclaration;
        readonly parent: ClassDeclaration | ClassExpression;
        readonly body: Block;
    }
    interface TypeNode extends Node {
        _typeNodeBrand: any;
    }
    interface KeywordTypeNode<TKind extends KeywordTypeSyntaxKind = KeywordTypeSyntaxKind> extends KeywordToken<TKind>, TypeNode {
        readonly kind: TKind;
    }
    interface ImportTypeAssertionContainer extends Node {
        readonly kind: SyntaxKind.ImportTypeAssertionContainer;
        readonly parent: ImportTypeNode;
readonly assertClause: AssertClause;
        readonly multiLine?: boolean;
    }
    interface ImportTypeNode extends NodeWithTypeArguments {
        readonly kind: SyntaxKind.ImportType;
        readonly isTypeOf: boolean;
        readonly argument: TypeNode;
readonly assertions?: ImportTypeAssertionContainer;
        readonly attributes?: ImportAttributes;
        readonly qualifier?: EntityName;
    }
    interface ThisTypeNode extends TypeNode {
        readonly kind: SyntaxKind.ThisType;
    }
    type FunctionOrConstructorTypeNode = FunctionTypeNode | ConstructorTypeNode;
    interface FunctionOrConstructorTypeNodeBase extends TypeNode, SignatureDeclarationBase {
        readonly kind: SyntaxKind.FunctionType | SyntaxKind.ConstructorType;
        readonly type: TypeNode;
    }
    interface FunctionTypeNode extends FunctionOrConstructorTypeNodeBase, LocalsContainer {
        readonly kind: SyntaxKind.FunctionType;
    }
    interface ConstructorTypeNode extends FunctionOrConstructorTypeNodeBase, LocalsContainer {
        readonly kind: SyntaxKind.ConstructorType;
        readonly modifiers?: NodeArray<Modifier>;
    }
    interface NodeWithTypeArguments extends TypeNode {
        readonly typeArguments?: NodeArray<TypeNode>;
    }
    type TypeReferenceType = TypeReferenceNode | ExpressionWithTypeArguments;
    interface TypeReferenceNode extends NodeWithTypeArguments {
        readonly kind: SyntaxKind.TypeReference;
        readonly typeName: EntityName;
    }
    interface TypePredicateNode extends TypeNode {
        readonly kind: SyntaxKind.TypePredicate;
        readonly parent: SignatureDeclaration | JSDocTypeExpression;
        readonly assertsModifier?: AssertsKeyword;
        readonly parameterName: Identifier | ThisTypeNode;
        readonly type?: TypeNode;
    }
    interface TypeQueryNode extends NodeWithTypeArguments {
        readonly kind: SyntaxKind.TypeQuery;
        readonly exprName: EntityName;
    }
    interface TypeLiteralNode extends TypeNode, Declaration {
        readonly kind: SyntaxKind.TypeLiteral;
        readonly members: NodeArray<TypeElement>;
    }
    interface ArrayTypeNode extends TypeNode {
        readonly kind: SyntaxKind.ArrayType;
        readonly elementType: TypeNode;
    }
    interface TupleTypeNode extends TypeNode {
        readonly kind: SyntaxKind.TupleType;
        readonly elements: NodeArray<TypeNode | NamedTupleMember>;
    }
    interface NamedTupleMember extends TypeNode, Declaration, JSDocContainer {
        readonly kind: SyntaxKind.NamedTupleMember;
        readonly dotDotDotToken?: Token<SyntaxKind.DotDotDotToken>;
        readonly name: Identifier;
        readonly questionToken?: Token<SyntaxKind.QuestionToken>;
        readonly type: TypeNode;
    }
    interface OptionalTypeNode extends TypeNode {
        readonly kind: SyntaxKind.OptionalType;
        readonly type: TypeNode;
    }
    interface RestTypeNode extends TypeNode {
        readonly kind: SyntaxKind.RestType;
        readonly type: TypeNode;
    }
    type UnionOrIntersectionTypeNode = UnionTypeNode | IntersectionTypeNode;
    interface UnionTypeNode extends TypeNode {
        readonly kind: SyntaxKind.UnionType;
        readonly types: NodeArray<TypeNode>;
    }
    interface IntersectionTypeNode extends TypeNode {
        readonly kind: SyntaxKind.IntersectionType;
        readonly types: NodeArray<TypeNode>;
    }
    interface ConditionalTypeNode extends TypeNode, LocalsContainer {
        readonly kind: SyntaxKind.ConditionalType;
        readonly checkType: TypeNode;
        readonly extendsType: TypeNode;
        readonly trueType: TypeNode;
        readonly falseType: TypeNode;
    }
    interface InferTypeNode extends TypeNode {
        readonly kind: SyntaxKind.InferType;
        readonly typeParameter: TypeParameterDeclaration;
    }
    interface ParenthesizedTypeNode extends TypeNode {
        readonly kind: SyntaxKind.ParenthesizedType;
        readonly type: TypeNode;
    }
    interface TypeOperatorNode extends TypeNode {
        readonly kind: SyntaxKind.TypeOperator;
        readonly operator: SyntaxKind.KeyOfKeyword | SyntaxKind.UniqueKeyword | SyntaxKind.ReadonlyKeyword;
        readonly type: TypeNode;
    }
    interface IndexedAccessTypeNode extends TypeNode {
        readonly kind: SyntaxKind.IndexedAccessType;
        readonly objectType: TypeNode;
        readonly indexType: TypeNode;
    }
    interface MappedTypeNode extends TypeNode, Declaration, LocalsContainer {
        readonly kind: SyntaxKind.MappedType;
        readonly readonlyToken?: ReadonlyKeyword | PlusToken | MinusToken;
        readonly typeParameter: TypeParameterDeclaration;
        readonly nameType?: TypeNode;
        readonly questionToken?: QuestionToken | PlusToken | MinusToken;
        readonly type?: TypeNode;
        readonly members?: NodeArray<TypeElement>;
    }
    interface LiteralTypeNode extends TypeNode {
        readonly kind: SyntaxKind.LiteralType;
        readonly literal: NullLiteral | BooleanLiteral | LiteralExpression | PrefixUnaryExpression;
    }
    interface StringLiteral extends LiteralExpression, Declaration {
        readonly kind: SyntaxKind.StringLiteral;
    }
    type StringLiteralLike = StringLiteral | NoSubstitutionTemplateLiteral;
    type PropertyNameLiteral = Identifier | StringLiteralLike | NumericLiteral | JsxNamespacedName | BigIntLiteral;
    interface TemplateLiteralTypeNode extends TypeNode {
        kind: SyntaxKind.TemplateLiteralType;
        readonly head: TemplateHead;
        readonly templateSpans: NodeArray<TemplateLiteralTypeSpan>;
    }
    interface TemplateLiteralTypeSpan extends TypeNode {
        readonly kind: SyntaxKind.TemplateLiteralTypeSpan;
        readonly parent: TemplateLiteralTypeNode;
        readonly type: TypeNode;
        readonly literal: TemplateMiddle | TemplateTail;
    }
    interface Expression extends Node {
        _expressionBrand: any;
    }
    interface OmittedExpression extends Expression {
        readonly kind: SyntaxKind.OmittedExpression;
    }
    interface PartiallyEmittedExpression extends LeftHandSideExpression {
        readonly kind: SyntaxKind.PartiallyEmittedExpression;
        readonly expression: Expression;
    }
    interface UnaryExpression extends Expression {
        _unaryExpressionBrand: any;
    }
    type IncrementExpression = UpdateExpression;
    interface UpdateExpression extends UnaryExpression {
        _updateExpressionBrand: any;
    }
    type PrefixUnaryOperator = SyntaxKind.PlusPlusToken | SyntaxKind.MinusMinusToken | SyntaxKind.PlusToken | SyntaxKind.MinusToken | SyntaxKind.TildeToken | SyntaxKind.ExclamationToken;
    interface PrefixUnaryExpression extends UpdateExpression {
        readonly kind: SyntaxKind.PrefixUnaryExpression;
        readonly operator: PrefixUnaryOperator;
        readonly operand: UnaryExpression;
    }
    type PostfixUnaryOperator = SyntaxKind.PlusPlusToken | SyntaxKind.MinusMinusToken;
    interface PostfixUnaryExpression extends UpdateExpression {
        readonly kind: SyntaxKind.PostfixUnaryExpression;
        readonly operand: LeftHandSideExpression;
        readonly operator: PostfixUnaryOperator;
    }
    interface LeftHandSideExpression extends UpdateExpression {
        _leftHandSideExpressionBrand: any;
    }
    interface MemberExpression extends LeftHandSideExpression {
        _memberExpressionBrand: any;
    }
    interface PrimaryExpression extends MemberExpression {
        _primaryExpressionBrand: any;
    }
    interface NullLiteral extends PrimaryExpression {
        readonly kind: SyntaxKind.NullKeyword;
    }
    interface TrueLiteral extends PrimaryExpression {
        readonly kind: SyntaxKind.TrueKeyword;
    }
    interface FalseLiteral extends PrimaryExpression {
        readonly kind: SyntaxKind.FalseKeyword;
    }
    type BooleanLiteral = TrueLiteral | FalseLiteral;
    interface ThisExpression extends PrimaryExpression, FlowContainer {
        readonly kind: SyntaxKind.ThisKeyword;
    }
    interface SuperExpression extends PrimaryExpression, FlowContainer {
        readonly kind: SyntaxKind.SuperKeyword;
    }
    interface ImportExpression extends PrimaryExpression {
        readonly kind: SyntaxKind.ImportKeyword;
    }
    interface DeleteExpression extends UnaryExpression {
        readonly kind: SyntaxKind.DeleteExpression;
        readonly expression: UnaryExpression;
    }
    interface TypeOfExpression extends UnaryExpression {
        readonly kind: SyntaxKind.TypeOfExpression;
        readonly expression: UnaryExpression;
    }
    interface VoidExpression extends UnaryExpression {
        readonly kind: SyntaxKind.VoidExpression;
        readonly expression: UnaryExpression;
    }
    interface AwaitExpression extends UnaryExpression {
        readonly kind: SyntaxKind.AwaitExpression;
        readonly expression: UnaryExpression;
    }
    interface YieldExpression extends Expression {
        readonly kind: SyntaxKind.YieldExpression;
        readonly asteriskToken?: AsteriskToken;
        readonly expression?: Expression;
    }
    interface SyntheticExpression extends Expression {
        readonly kind: SyntaxKind.SyntheticExpression;
        readonly isSpread: boolean;
        readonly type: Type;
        readonly tupleNameSource?: ParameterDeclaration | NamedTupleMember;
    }
    type ExponentiationOperator = SyntaxKind.AsteriskAsteriskToken;
    type MultiplicativeOperator = SyntaxKind.AsteriskToken | SyntaxKind.SlashToken | SyntaxKind.PercentToken;
    type MultiplicativeOperatorOrHigher = ExponentiationOperator | MultiplicativeOperator;
    type AdditiveOperator = SyntaxKind.PlusToken | SyntaxKind.MinusToken;
    type AdditiveOperatorOrHigher = MultiplicativeOperatorOrHigher | AdditiveOperator;
    type ShiftOperator = SyntaxKind.LessThanLessThanToken | SyntaxKind.GreaterThanGreaterThanToken | SyntaxKind.GreaterThanGreaterThanGreaterThanToken;
    type ShiftOperatorOrHigher = AdditiveOperatorOrHigher | ShiftOperator;
    type RelationalOperator = SyntaxKind.LessThanToken | SyntaxKind.LessThanEqualsToken | SyntaxKind.GreaterThanToken | SyntaxKind.GreaterThanEqualsToken | SyntaxKind.InstanceOfKeyword | SyntaxKind.InKeyword;
    type RelationalOperatorOrHigher = ShiftOperatorOrHigher | RelationalOperator;
    type EqualityOperator = SyntaxKind.EqualsEqualsToken | SyntaxKind.EqualsEqualsEqualsToken | SyntaxKind.ExclamationEqualsEqualsToken | SyntaxKind.ExclamationEqualsToken;
    type EqualityOperatorOrHigher = RelationalOperatorOrHigher | EqualityOperator;
    type BitwiseOperator = SyntaxKind.AmpersandToken | SyntaxKind.BarToken | SyntaxKind.CaretToken;
    type BitwiseOperatorOrHigher = EqualityOperatorOrHigher | BitwiseOperator;
    type LogicalOperator = SyntaxKind.AmpersandAmpersandToken | SyntaxKind.BarBarToken;
    type LogicalOperatorOrHigher = BitwiseOperatorOrHigher | LogicalOperator;
    type CompoundAssignmentOperator = SyntaxKind.PlusEqualsToken | SyntaxKind.MinusEqualsToken | SyntaxKind.AsteriskAsteriskEqualsToken | SyntaxKind.AsteriskEqualsToken | SyntaxKind.SlashEqualsToken | SyntaxKind.PercentEqualsToken | SyntaxKind.AmpersandEqualsToken | SyntaxKind.BarEqualsToken | SyntaxKind.CaretEqualsToken | SyntaxKind.LessThanLessThanEqualsToken | SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken | SyntaxKind.GreaterThanGreaterThanEqualsToken | SyntaxKind.BarBarEqualsToken | SyntaxKind.AmpersandAmpersandEqualsToken | SyntaxKind.QuestionQuestionEqualsToken;
    type AssignmentOperator = SyntaxKind.EqualsToken | CompoundAssignmentOperator;
    type AssignmentOperatorOrHigher = SyntaxKind.QuestionQuestionToken | LogicalOperatorOrHigher | AssignmentOperator;
    type BinaryOperator = AssignmentOperatorOrHigher | SyntaxKind.CommaToken;
    type LogicalOrCoalescingAssignmentOperator = SyntaxKind.AmpersandAmpersandEqualsToken | SyntaxKind.BarBarEqualsToken | SyntaxKind.QuestionQuestionEqualsToken;
    type BinaryOperatorToken = Token<BinaryOperator>;
    interface BinaryExpression extends Expression, Declaration, JSDocContainer {
        readonly kind: SyntaxKind.BinaryExpression;
        readonly left: Expression;
        readonly operatorToken: BinaryOperatorToken;
        readonly right: Expression;
    }
    type AssignmentOperatorToken = Token<AssignmentOperator>;
    interface AssignmentExpression<TOperator extends AssignmentOperatorToken> extends BinaryExpression {
        readonly left: LeftHandSideExpression;
        readonly operatorToken: TOperator;
    }
    interface ObjectDestructuringAssignment extends AssignmentExpression<EqualsToken> {
        readonly left: ObjectLiteralExpression;
    }
    interface ArrayDestructuringAssignment extends AssignmentExpression<EqualsToken> {
        readonly left: ArrayLiteralExpression;
    }
    type DestructuringAssignment = ObjectDestructuringAssignment | ArrayDestructuringAssignment;
    type BindingOrAssignmentElement = VariableDeclaration | ParameterDeclaration | ObjectBindingOrAssignmentElement | ArrayBindingOrAssignmentElement;
    type ObjectBindingOrAssignmentElement = BindingElement | PropertyAssignment | ShorthandPropertyAssignment | SpreadAssignment;
    type ArrayBindingOrAssignmentElement = BindingElement | OmittedExpression | SpreadElement | ArrayLiteralExpression | ObjectLiteralExpression | AssignmentExpression<EqualsToken> | Identifier | PropertyAccessExpression | ElementAccessExpression;
    type BindingOrAssignmentElementRestIndicator = DotDotDotToken | SpreadElement | SpreadAssignment;
    type BindingOrAssignmentElementTarget = BindingOrAssignmentPattern | Identifier | PropertyAccessExpression | ElementAccessExpression | OmittedExpression;
    type ObjectBindingOrAssignmentPattern = ObjectBindingPattern | ObjectLiteralExpression;
    type ArrayBindingOrAssignmentPattern = ArrayBindingPattern | ArrayLiteralExpression;
    type AssignmentPattern = ObjectLiteralExpression | ArrayLiteralExpression;
    type BindingOrAssignmentPattern = ObjectBindingOrAssignmentPattern | ArrayBindingOrAssignmentPattern;
    interface ConditionalExpression extends Expression {
        readonly kind: SyntaxKind.ConditionalExpression;
        readonly condition: Expression;
        readonly questionToken: QuestionToken;
        readonly whenTrue: Expression;
        readonly colonToken: ColonToken;
        readonly whenFalse: Expression;
    }
    type FunctionBody = Block;
    type ConciseBody = FunctionBody | Expression;
    interface FunctionExpression extends PrimaryExpression, FunctionLikeDeclarationBase, JSDocContainer, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.FunctionExpression;
        readonly modifiers?: NodeArray<Modifier>;
        readonly name?: Identifier;
        readonly body: FunctionBody;
    }
    interface ArrowFunction extends Expression, FunctionLikeDeclarationBase, JSDocContainer, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.ArrowFunction;
        readonly modifiers?: NodeArray<Modifier>;
        readonly equalsGreaterThanToken: EqualsGreaterThanToken;
        readonly body: ConciseBody;
        readonly name: never;
    }
    interface LiteralLikeNode extends Node {
        text: string;
        isUnterminated?: boolean;
        hasExtendedUnicodeEscape?: boolean;
    }
    interface TemplateLiteralLikeNode extends LiteralLikeNode {
        rawText?: string;
    }
    interface LiteralExpression extends LiteralLikeNode, PrimaryExpression {
        _literalExpressionBrand: any;
    }
    interface RegularExpressionLiteral extends LiteralExpression {
        readonly kind: SyntaxKind.RegularExpressionLiteral;
    }
    interface NoSubstitutionTemplateLiteral extends LiteralExpression, TemplateLiteralLikeNode, Declaration {
        readonly kind: SyntaxKind.NoSubstitutionTemplateLiteral;
    }
    enum TokenFlags {
        None = 0,
        Scientific = 16,
        Octal = 32,
        HexSpecifier = 64,
        BinarySpecifier = 128,
        OctalSpecifier = 256,
    }
    interface NumericLiteral extends LiteralExpression, Declaration {
        readonly kind: SyntaxKind.NumericLiteral;
    }
    interface BigIntLiteral extends LiteralExpression {
        readonly kind: SyntaxKind.BigIntLiteral;
    }
    type LiteralToken = NumericLiteral | BigIntLiteral | StringLiteral | JsxText | RegularExpressionLiteral | NoSubstitutionTemplateLiteral;
    interface TemplateHead extends TemplateLiteralLikeNode {
        readonly kind: SyntaxKind.TemplateHead;
        readonly parent: TemplateExpression | TemplateLiteralTypeNode;
    }
    interface TemplateMiddle extends TemplateLiteralLikeNode {
        readonly kind: SyntaxKind.TemplateMiddle;
        readonly parent: TemplateSpan | TemplateLiteralTypeSpan;
    }
    interface TemplateTail extends TemplateLiteralLikeNode {
        readonly kind: SyntaxKind.TemplateTail;
        readonly parent: TemplateSpan | TemplateLiteralTypeSpan;
    }
    type PseudoLiteralToken = TemplateHead | TemplateMiddle | TemplateTail;
    type TemplateLiteralToken = NoSubstitutionTemplateLiteral | PseudoLiteralToken;
    interface TemplateExpression extends PrimaryExpression {
        readonly kind: SyntaxKind.TemplateExpression;
        readonly head: TemplateHead;
        readonly templateSpans: NodeArray<TemplateSpan>;
    }
    type TemplateLiteral = TemplateExpression | NoSubstitutionTemplateLiteral;
    interface TemplateSpan extends Node {
        readonly kind: SyntaxKind.TemplateSpan;
        readonly parent: TemplateExpression;
        readonly expression: Expression;
        readonly literal: TemplateMiddle | TemplateTail;
    }
    interface ParenthesizedExpression extends PrimaryExpression, JSDocContainer {
        readonly kind: SyntaxKind.ParenthesizedExpression;
        readonly expression: Expression;
    }
    interface ArrayLiteralExpression extends PrimaryExpression {
        readonly kind: SyntaxKind.ArrayLiteralExpression;
        readonly elements: NodeArray<Expression>;
    }
    interface SpreadElement extends Expression {
        readonly kind: SyntaxKind.SpreadElement;
        readonly parent: ArrayLiteralExpression | CallExpression | NewExpression;
        readonly expression: Expression;
    }
    interface ObjectLiteralExpressionBase<T extends ObjectLiteralElement> extends PrimaryExpression, Declaration {
        readonly properties: NodeArray<T>;
    }
    interface ObjectLiteralExpression extends ObjectLiteralExpressionBase<ObjectLiteralElementLike>, JSDocContainer {
        readonly kind: SyntaxKind.ObjectLiteralExpression;
    }
    type EntityNameExpression = Identifier | PropertyAccessEntityNameExpression;
    type EntityNameOrEntityNameExpression = EntityName | EntityNameExpression;
    type AccessExpression = PropertyAccessExpression | ElementAccessExpression;
    interface PropertyAccessExpression extends MemberExpression, NamedDeclaration, JSDocContainer, FlowContainer {
        readonly kind: SyntaxKind.PropertyAccessExpression;
        readonly expression: LeftHandSideExpression;
        readonly questionDotToken?: QuestionDotToken;
        readonly name: MemberName;
    }
    interface PropertyAccessChain extends PropertyAccessExpression {
        _optionalChainBrand: any;
        readonly name: MemberName;
    }
    interface SuperPropertyAccessExpression extends PropertyAccessExpression {
        readonly expression: SuperExpression;
    }
    interface PropertyAccessEntityNameExpression extends PropertyAccessExpression {
        _propertyAccessExpressionLikeQualifiedNameBrand?: any;
        readonly expression: EntityNameExpression;
        readonly name: Identifier;
    }
    interface ElementAccessExpression extends MemberExpression, Declaration, JSDocContainer, FlowContainer {
        readonly kind: SyntaxKind.ElementAccessExpression;
        readonly expression: LeftHandSideExpression;
        readonly questionDotToken?: QuestionDotToken;
        readonly argumentExpression: Expression;
    }
    interface ElementAccessChain extends ElementAccessExpression {
        _optionalChainBrand: any;
    }
    interface SuperElementAccessExpression extends ElementAccessExpression {
        readonly expression: SuperExpression;
    }
    type SuperProperty = SuperPropertyAccessExpression | SuperElementAccessExpression;
    interface CallExpression extends LeftHandSideExpression, Declaration {
        readonly kind: SyntaxKind.CallExpression;
        readonly expression: LeftHandSideExpression;
        readonly questionDotToken?: QuestionDotToken;
        readonly typeArguments?: NodeArray<TypeNode>;
        readonly arguments: NodeArray<Expression>;
    }
    interface CallChain extends CallExpression {
        _optionalChainBrand: any;
    }
    type OptionalChain = PropertyAccessChain | ElementAccessChain | CallChain | NonNullChain;
    interface SuperCall extends CallExpression {
        readonly expression: SuperExpression;
    }
    interface ImportCall extends CallExpression {
        readonly expression: ImportExpression | ImportDeferProperty;
    }
    interface ExpressionWithTypeArguments extends MemberExpression, NodeWithTypeArguments {
        readonly kind: SyntaxKind.ExpressionWithTypeArguments;
        readonly expression: LeftHandSideExpression;
    }
    interface NewExpression extends PrimaryExpression, Declaration {
        readonly kind: SyntaxKind.NewExpression;
        readonly expression: LeftHandSideExpression;
        readonly typeArguments?: NodeArray<TypeNode>;
        readonly arguments?: NodeArray<Expression>;
    }
    interface TaggedTemplateExpression extends MemberExpression {
        readonly kind: SyntaxKind.TaggedTemplateExpression;
        readonly tag: LeftHandSideExpression;
        readonly typeArguments?: NodeArray<TypeNode>;
        readonly template: TemplateLiteral;
    }
    interface InstanceofExpression extends BinaryExpression {
        readonly operatorToken: Token<SyntaxKind.InstanceOfKeyword>;
    }
    type CallLikeExpression = CallExpression | NewExpression | TaggedTemplateExpression | Decorator | JsxCallLike | InstanceofExpression;
    interface AsExpression extends Expression {
        readonly kind: SyntaxKind.AsExpression;
        readonly expression: Expression;
        readonly type: TypeNode;
    }
    interface TypeAssertion extends UnaryExpression {
        readonly kind: SyntaxKind.TypeAssertionExpression;
        readonly type: TypeNode;
        readonly expression: UnaryExpression;
    }
    interface SatisfiesExpression extends Expression {
        readonly kind: SyntaxKind.SatisfiesExpression;
        readonly expression: Expression;
        readonly type: TypeNode;
    }
    type AssertionExpression = TypeAssertion | AsExpression;
    interface NonNullExpression extends LeftHandSideExpression {
        readonly kind: SyntaxKind.NonNullExpression;
        readonly expression: Expression;
    }
    interface NonNullChain extends NonNullExpression {
        _optionalChainBrand: any;
    }
    interface MetaProperty extends PrimaryExpression, FlowContainer {
        readonly kind: SyntaxKind.MetaProperty;
        readonly keywordToken: SyntaxKind.NewKeyword | SyntaxKind.ImportKeyword;
        readonly name: Identifier;
    }
    interface ImportDeferProperty extends MetaProperty {
        readonly keywordToken: SyntaxKind.ImportKeyword;
        readonly name: Identifier & {
            readonly escapedText: __String & "defer";
        };
    }
    interface JsxElement extends PrimaryExpression {
        readonly kind: SyntaxKind.JsxElement;
        readonly openingElement: JsxOpeningElement;
        readonly children: NodeArray<JsxChild>;
        readonly closingElement: JsxClosingElement;
    }
    type JsxOpeningLikeElement = JsxSelfClosingElement | JsxOpeningElement;
    type JsxCallLike = JsxOpeningLikeElement | JsxOpeningFragment;
    type JsxAttributeLike = JsxAttribute | JsxSpreadAttribute;
    type JsxAttributeName = Identifier | JsxNamespacedName;
    type JsxTagNameExpression = Identifier | ThisExpression | JsxTagNamePropertyAccess | JsxNamespacedName;
    interface JsxTagNamePropertyAccess extends PropertyAccessExpression {
        readonly expression: Identifier | ThisExpression | JsxTagNamePropertyAccess;
    }
    interface JsxAttributes extends PrimaryExpression, Declaration {
        readonly properties: NodeArray<JsxAttributeLike>;
        readonly kind: SyntaxKind.JsxAttributes;
        readonly parent: JsxOpeningLikeElement;
    }
    interface JsxNamespacedName extends Node {
        readonly kind: SyntaxKind.JsxNamespacedName;
        readonly name: Identifier;
        readonly namespace: Identifier;
    }
    interface JsxOpeningElement extends Expression {
        readonly kind: SyntaxKind.JsxOpeningElement;
        readonly parent: JsxElement;
        readonly tagName: JsxTagNameExpression;
        readonly typeArguments?: NodeArray<TypeNode>;
        readonly attributes: JsxAttributes;
    }
    interface JsxSelfClosingElement extends PrimaryExpression {
        readonly kind: SyntaxKind.JsxSelfClosingElement;
        readonly tagName: JsxTagNameExpression;
        readonly typeArguments?: NodeArray<TypeNode>;
        readonly attributes: JsxAttributes;
    }
    interface JsxFragment extends PrimaryExpression {
        readonly kind: SyntaxKind.JsxFragment;
        readonly openingFragment: JsxOpeningFragment;
        readonly children: NodeArray<JsxChild>;
        readonly closingFragment: JsxClosingFragment;
    }
    interface JsxOpeningFragment extends Expression {
        readonly kind: SyntaxKind.JsxOpeningFragment;
        readonly parent: JsxFragment;
    }
    interface JsxClosingFragment extends Expression {
        readonly kind: SyntaxKind.JsxClosingFragment;
        readonly parent: JsxFragment;
    }
    interface JsxAttribute extends Declaration {
        readonly kind: SyntaxKind.JsxAttribute;
        readonly parent: JsxAttributes;
        readonly name: JsxAttributeName;
        readonly initializer?: JsxAttributeValue;
    }
    type JsxAttributeValue = StringLiteral | JsxExpression | JsxElement | JsxSelfClosingElement | JsxFragment;
    interface JsxSpreadAttribute extends ObjectLiteralElement {
        readonly kind: SyntaxKind.JsxSpreadAttribute;
        readonly parent: JsxAttributes;
        readonly expression: Expression;
    }
    interface JsxClosingElement extends Node {
        readonly kind: SyntaxKind.JsxClosingElement;
        readonly parent: JsxElement;
        readonly tagName: JsxTagNameExpression;
    }
    interface JsxExpression extends Expression {
        readonly kind: SyntaxKind.JsxExpression;
        readonly parent: JsxElement | JsxFragment | JsxAttributeLike;
        readonly dotDotDotToken?: Token<SyntaxKind.DotDotDotToken>;
        readonly expression?: Expression;
    }
    interface JsxText extends LiteralLikeNode {
        readonly kind: SyntaxKind.JsxText;
        readonly parent: JsxElement | JsxFragment;
        readonly containsOnlyTriviaWhiteSpaces: boolean;
    }
    type JsxChild = JsxText | JsxExpression | JsxElement | JsxSelfClosingElement | JsxFragment;
    interface Statement extends Node, JSDocContainer {
        _statementBrand: any;
    }
    interface NotEmittedStatement extends Statement {
        readonly kind: SyntaxKind.NotEmittedStatement;
    }
    interface NotEmittedTypeElement extends TypeElement {
        readonly kind: SyntaxKind.NotEmittedTypeElement;
    }
    interface CommaListExpression extends Expression {
        readonly kind: SyntaxKind.CommaListExpression;
        readonly elements: NodeArray<Expression>;
    }
    interface EmptyStatement extends Statement {
        readonly kind: SyntaxKind.EmptyStatement;
    }
    interface DebuggerStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.DebuggerStatement;
    }
    interface MissingDeclaration extends DeclarationStatement, PrimaryExpression {
        readonly kind: SyntaxKind.MissingDeclaration;
        readonly name?: Identifier;
    }
    type BlockLike = SourceFile | Block | ModuleBlock | CaseOrDefaultClause;
    interface Block extends Statement, LocalsContainer {
        readonly kind: SyntaxKind.Block;
        readonly statements: NodeArray<Statement>;
    }
    interface VariableStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.VariableStatement;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly declarationList: VariableDeclarationList;
    }
    interface ExpressionStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.ExpressionStatement;
        readonly expression: Expression;
    }
    interface IfStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.IfStatement;
        readonly expression: Expression;
        readonly thenStatement: Statement;
        readonly elseStatement?: Statement;
    }
    interface IterationStatement extends Statement {
        readonly statement: Statement;
    }
    interface DoStatement extends IterationStatement, FlowContainer {
        readonly kind: SyntaxKind.DoStatement;
        readonly expression: Expression;
    }
    interface WhileStatement extends IterationStatement, FlowContainer {
        readonly kind: SyntaxKind.WhileStatement;
        readonly expression: Expression;
    }
    type ForInitializer = VariableDeclarationList | Expression;
    interface ForStatement extends IterationStatement, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.ForStatement;
        readonly initializer?: ForInitializer;
        readonly condition?: Expression;
        readonly incrementor?: Expression;
    }
    type ForInOrOfStatement = ForInStatement | ForOfStatement;
    interface ForInStatement extends IterationStatement, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.ForInStatement;
        readonly initializer: ForInitializer;
        readonly expression: Expression;
    }
    interface ForOfStatement extends IterationStatement, LocalsContainer, FlowContainer {
        readonly kind: SyntaxKind.ForOfStatement;
        readonly awaitModifier?: AwaitKeyword;
        readonly initializer: ForInitializer;
        readonly expression: Expression;
    }
    interface BreakStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.BreakStatement;
        readonly label?: Identifier;
    }
    interface ContinueStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.ContinueStatement;
        readonly label?: Identifier;
    }
    type BreakOrContinueStatement = BreakStatement | ContinueStatement;
    interface ReturnStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.ReturnStatement;
        readonly expression?: Expression;
    }
    interface WithStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.WithStatement;
        readonly expression: Expression;
        readonly statement: Statement;
    }
    interface SwitchStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.SwitchStatement;
        readonly expression: Expression;
        readonly caseBlock: CaseBlock;
        possiblyExhaustive?: boolean;
    }
    interface CaseBlock extends Node, LocalsContainer {
        readonly kind: SyntaxKind.CaseBlock;
        readonly parent: SwitchStatement;
        readonly clauses: NodeArray<CaseOrDefaultClause>;
    }
    interface CaseClause extends Node, JSDocContainer {
        readonly kind: SyntaxKind.CaseClause;
        readonly parent: CaseBlock;
        readonly expression: Expression;
        readonly statements: NodeArray<Statement>;
    }
    interface DefaultClause extends Node {
        readonly kind: SyntaxKind.DefaultClause;
        readonly parent: CaseBlock;
        readonly statements: NodeArray<Statement>;
    }
    type CaseOrDefaultClause = CaseClause | DefaultClause;
    interface LabeledStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.LabeledStatement;
        readonly label: Identifier;
        readonly statement: Statement;
    }
    interface ThrowStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.ThrowStatement;
        readonly expression: Expression;
    }
    interface TryStatement extends Statement, FlowContainer {
        readonly kind: SyntaxKind.TryStatement;
        readonly tryBlock: Block;
        readonly catchClause?: CatchClause;
        readonly finallyBlock?: Block;
    }
    interface CatchClause extends Node, LocalsContainer {
        readonly kind: SyntaxKind.CatchClause;
        readonly parent: TryStatement;
        readonly variableDeclaration?: VariableDeclaration;
        readonly block: Block;
    }
    type ObjectTypeDeclaration = ClassLikeDeclaration | InterfaceDeclaration | TypeLiteralNode;
    type DeclarationWithTypeParameters = DeclarationWithTypeParameterChildren | JSDocTypedefTag | JSDocCallbackTag | JSDocSignature;
    type DeclarationWithTypeParameterChildren = SignatureDeclaration | ClassLikeDeclaration | InterfaceDeclaration | TypeAliasDeclaration | JSDocTemplateTag;
    interface ClassLikeDeclarationBase extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.ClassDeclaration | SyntaxKind.ClassExpression;
        readonly name?: Identifier;
        readonly typeParameters?: NodeArray<TypeParameterDeclaration>;
        readonly heritageClauses?: NodeArray<HeritageClause>;
        readonly members: NodeArray<ClassElement>;
    }
    interface ClassDeclaration extends ClassLikeDeclarationBase, DeclarationStatement {
        readonly kind: SyntaxKind.ClassDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name?: Identifier;
    }
    interface ClassExpression extends ClassLikeDeclarationBase, PrimaryExpression {
        readonly kind: SyntaxKind.ClassExpression;
        readonly modifiers?: NodeArray<ModifierLike>;
    }
    type ClassLikeDeclaration = ClassDeclaration | ClassExpression;
    interface ClassElement extends NamedDeclaration {
        _classElementBrand: any;
        readonly name?: PropertyName;
    }
    interface TypeElement extends NamedDeclaration {
        _typeElementBrand: any;
        readonly name?: PropertyName;
        readonly questionToken?: QuestionToken | undefined;
    }
    interface InterfaceDeclaration extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.InterfaceDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: Identifier;
        readonly typeParameters?: NodeArray<TypeParameterDeclaration>;
        readonly heritageClauses?: NodeArray<HeritageClause>;
        readonly members: NodeArray<TypeElement>;
    }
    interface HeritageClause extends Node {
        readonly kind: SyntaxKind.HeritageClause;
        readonly parent: InterfaceDeclaration | ClassLikeDeclaration;
        readonly token: SyntaxKind.ExtendsKeyword | SyntaxKind.ImplementsKeyword;
        readonly types: NodeArray<ExpressionWithTypeArguments>;
    }
    interface TypeAliasDeclaration extends DeclarationStatement, JSDocContainer, LocalsContainer {
        readonly kind: SyntaxKind.TypeAliasDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: Identifier;
        readonly typeParameters?: NodeArray<TypeParameterDeclaration>;
        readonly type: TypeNode;
    }
    interface EnumMember extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.EnumMember;
        readonly parent: EnumDeclaration;
        readonly name: PropertyName;
        readonly initializer?: Expression;
    }
    interface EnumDeclaration extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.EnumDeclaration;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: Identifier;
        readonly members: NodeArray<EnumMember>;
    }
    type ModuleName = Identifier | StringLiteral;
    type ModuleBody = NamespaceBody | JSDocNamespaceBody;
    interface ModuleDeclaration extends DeclarationStatement, JSDocContainer, LocalsContainer {
        readonly kind: SyntaxKind.ModuleDeclaration;
        readonly parent: ModuleBody | SourceFile;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: ModuleName;
        readonly body?: ModuleBody | JSDocNamespaceDeclaration;
    }
    type NamespaceBody = ModuleBlock | NamespaceDeclaration;
    interface NamespaceDeclaration extends ModuleDeclaration {
        readonly name: Identifier;
        readonly body: NamespaceBody;
    }
    type JSDocNamespaceBody = Identifier | JSDocNamespaceDeclaration;
    interface JSDocNamespaceDeclaration extends ModuleDeclaration {
        readonly name: Identifier;
        readonly body?: JSDocNamespaceBody;
    }
    interface ModuleBlock extends Node, Statement {
        readonly kind: SyntaxKind.ModuleBlock;
        readonly parent: ModuleDeclaration;
        readonly statements: NodeArray<Statement>;
    }
    type ModuleReference = EntityName | ExternalModuleReference;
    interface ImportEqualsDeclaration extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.ImportEqualsDeclaration;
        readonly parent: SourceFile | ModuleBlock;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly name: Identifier;
        readonly isTypeOnly: boolean;
        readonly moduleReference: ModuleReference;
    }
    interface ExternalModuleReference extends Node {
        readonly kind: SyntaxKind.ExternalModuleReference;
        readonly parent: ImportEqualsDeclaration;
        readonly expression: Expression;
    }
    interface ImportDeclaration extends Statement {
        readonly kind: SyntaxKind.ImportDeclaration;
        readonly parent: SourceFile | ModuleBlock;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly importClause?: ImportClause;
        readonly moduleSpecifier: Expression;
readonly assertClause?: AssertClause;
        readonly attributes?: ImportAttributes;
    }
    type NamedImportBindings = NamespaceImport | NamedImports;
    type NamedExportBindings = NamespaceExport | NamedExports;
    interface ImportClause extends NamedDeclaration {
        readonly kind: SyntaxKind.ImportClause;
        readonly parent: ImportDeclaration | JSDocImportTag;
        readonly isTypeOnly: boolean;
        readonly phaseModifier: undefined | ImportPhaseModifierSyntaxKind;
        readonly name?: Identifier;
        readonly namedBindings?: NamedImportBindings;
    }
    type ImportPhaseModifierSyntaxKind = SyntaxKind.TypeKeyword | SyntaxKind.DeferKeyword;
    type AssertionKey = ImportAttributeName;
    interface AssertEntry extends ImportAttribute {
    }
    interface AssertClause extends ImportAttributes {
    }
    type ImportAttributeName = Identifier | StringLiteral;
    interface ImportAttribute extends Node {
        readonly kind: SyntaxKind.ImportAttribute;
        readonly parent: ImportAttributes;
        readonly name: ImportAttributeName;
        readonly value: Expression;
    }
    interface ImportAttributes extends Node {
        readonly token: SyntaxKind.WithKeyword | SyntaxKind.AssertKeyword;
        readonly kind: SyntaxKind.ImportAttributes;
        readonly parent: ImportDeclaration | ExportDeclaration;
        readonly elements: NodeArray<ImportAttribute>;
        readonly multiLine?: boolean;
    }
    interface NamespaceImport extends NamedDeclaration {
        readonly kind: SyntaxKind.NamespaceImport;
        readonly parent: ImportClause;
        readonly name: Identifier;
    }
    interface NamespaceExport extends NamedDeclaration {
        readonly kind: SyntaxKind.NamespaceExport;
        readonly parent: ExportDeclaration;
        readonly name: ModuleExportName;
    }
    interface NamespaceExportDeclaration extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.NamespaceExportDeclaration;
        readonly name: Identifier;
    }
    interface ExportDeclaration extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.ExportDeclaration;
        readonly parent: SourceFile | ModuleBlock;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly isTypeOnly: boolean;
        readonly exportClause?: NamedExportBindings;
        readonly moduleSpecifier?: Expression;
readonly assertClause?: AssertClause;
        readonly attributes?: ImportAttributes;
    }
    interface NamedImports extends Node {
        readonly kind: SyntaxKind.NamedImports;
        readonly parent: ImportClause;
        readonly elements: NodeArray<ImportSpecifier>;
    }
    interface NamedExports extends Node {
        readonly kind: SyntaxKind.NamedExports;
        readonly parent: ExportDeclaration;
        readonly elements: NodeArray<ExportSpecifier>;
    }
    type NamedImportsOrExports = NamedImports | NamedExports;
    interface ImportSpecifier extends NamedDeclaration {
        readonly kind: SyntaxKind.ImportSpecifier;
        readonly parent: NamedImports;
        readonly propertyName?: ModuleExportName;
        readonly name: Identifier;
        readonly isTypeOnly: boolean;
    }
    interface ExportSpecifier extends NamedDeclaration, JSDocContainer {
        readonly kind: SyntaxKind.ExportSpecifier;
        readonly parent: NamedExports;
        readonly isTypeOnly: boolean;
        readonly propertyName?: ModuleExportName;
        readonly name: ModuleExportName;
    }
    type ModuleExportName = Identifier | StringLiteral;
    type ImportOrExportSpecifier = ImportSpecifier | ExportSpecifier;
    type TypeOnlyCompatibleAliasDeclaration = ImportClause | ImportEqualsDeclaration | NamespaceImport | ImportOrExportSpecifier | ExportDeclaration | NamespaceExport;
    type TypeOnlyImportDeclaration =
        | ImportClause & {
            readonly isTypeOnly: true;
            readonly name: Identifier;
        }
        | ImportEqualsDeclaration & {
            readonly isTypeOnly: true;
        }
        | NamespaceImport & {
            readonly parent: ImportClause & {
                readonly isTypeOnly: true;
            };
        }
        | ImportSpecifier
            & ({
                readonly isTypeOnly: true;
            } | {
                readonly parent: NamedImports & {
                    readonly parent: ImportClause & {
                        readonly isTypeOnly: true;
                    };
                };
            });
    type TypeOnlyExportDeclaration =
        | ExportSpecifier
            & ({
                readonly isTypeOnly: true;
            } | {
                readonly parent: NamedExports & {
                    readonly parent: ExportDeclaration & {
                        readonly isTypeOnly: true;
                    };
                };
            })
        | ExportDeclaration & {
            readonly isTypeOnly: true;
            readonly moduleSpecifier: Expression;
        }
        | NamespaceExport & {
            readonly parent: ExportDeclaration & {
                readonly isTypeOnly: true;
                readonly moduleSpecifier: Expression;
            };
        };
    type TypeOnlyAliasDeclaration = TypeOnlyImportDeclaration | TypeOnlyExportDeclaration;
    interface ExportAssignment extends DeclarationStatement, JSDocContainer {
        readonly kind: SyntaxKind.ExportAssignment;
        readonly parent: SourceFile;
        readonly modifiers?: NodeArray<ModifierLike>;
        readonly isExportEquals?: boolean;
        readonly expression: Expression;
    }
    interface FileReference extends TextRange {
        fileName: string;
        resolutionMode?: ResolutionMode;
        preserve?: boolean;
    }
    interface CheckJsDirective extends TextRange {
        enabled: boolean;
    }
    type CommentKind = SyntaxKind.SingleLineCommentTrivia | SyntaxKind.MultiLineCommentTrivia;
    interface CommentRange extends TextRange {
        hasTrailingNewLine?: boolean;
        kind: CommentKind;
    }
    interface SynthesizedComment extends CommentRange {
        text: string;
        pos: -1;
        end: -1;
        hasLeadingNewline?: boolean;
    }
    interface JSDocTypeExpression extends TypeNode {
        readonly kind: SyntaxKind.JSDocTypeExpression;
        readonly type: TypeNode;
    }
    interface JSDocNameReference extends Node {
        readonly kind: SyntaxKind.JSDocNameReference;
        readonly name: EntityName | JSDocMemberName;
    }
    interface JSDocMemberName extends Node {
        readonly kind: SyntaxKind.JSDocMemberName;
        readonly left: EntityName | JSDocMemberName;
        readonly right: Identifier;
    }
    interface JSDocType extends TypeNode {
        _jsDocTypeBrand: any;
    }
    interface JSDocAllType extends JSDocType {
        readonly kind: SyntaxKind.JSDocAllType;
    }
    interface JSDocUnknownType extends JSDocType {
        readonly kind: SyntaxKind.JSDocUnknownType;
    }
    interface JSDocNonNullableType extends JSDocType {
        readonly kind: SyntaxKind.JSDocNonNullableType;
        readonly type: TypeNode;
        readonly postfix: boolean;
    }
    interface JSDocNullableType extends JSDocType {
        readonly kind: SyntaxKind.JSDocNullableType;
        readonly type: TypeNode;
        readonly postfix: boolean;
    }
    interface JSDocOptionalType extends JSDocType {
        readonly kind: SyntaxKind.JSDocOptionalType;
        readonly type: TypeNode;
    }
    interface JSDocFunctionType extends JSDocType, SignatureDeclarationBase, LocalsContainer {
        readonly kind: SyntaxKind.JSDocFunctionType;
    }
    interface JSDocVariadicType extends JSDocType {
        readonly kind: SyntaxKind.JSDocVariadicType;
        readonly type: TypeNode;
    }
    interface JSDocNamepathType extends JSDocType {
        readonly kind: SyntaxKind.JSDocNamepathType;
        readonly type: TypeNode;
    }
    type JSDocTypeReferencingNode = JSDocVariadicType | JSDocOptionalType | JSDocNullableType | JSDocNonNullableType;
    interface JSDoc extends Node {
        readonly kind: SyntaxKind.JSDoc;
        readonly parent: HasJSDoc;
        readonly tags?: NodeArray<JSDocTag>;
        readonly comment?: string | NodeArray<JSDocComment>;
    }
    interface JSDocTag extends Node {
        readonly parent: JSDoc | JSDocTypeLiteral;
        readonly tagName: Identifier;
        readonly comment?: string | NodeArray<JSDocComment>;
    }
    interface JSDocLink extends Node {
        readonly kind: SyntaxKind.JSDocLink;
        readonly name?: EntityName | JSDocMemberName;
        text: string;
    }
    interface JSDocLinkCode extends Node {
        readonly kind: SyntaxKind.JSDocLinkCode;
        readonly name?: EntityName | JSDocMemberName;
        text: string;
    }
    interface JSDocLinkPlain extends Node {
        readonly kind: SyntaxKind.JSDocLinkPlain;
        readonly name?: EntityName | JSDocMemberName;
        text: string;
    }
    type JSDocComment = JSDocText | JSDocLink | JSDocLinkCode | JSDocLinkPlain;
    interface JSDocText extends Node {
        readonly kind: SyntaxKind.JSDocText;
        text: string;
    }
    interface JSDocUnknownTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocTag;
    }
    interface JSDocAugmentsTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocAugmentsTag;
        readonly class: ExpressionWithTypeArguments & {
            readonly expression: Identifier | PropertyAccessEntityNameExpression;
        };
    }
    interface JSDocImplementsTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocImplementsTag;
        readonly class: ExpressionWithTypeArguments & {
            readonly expression: Identifier | PropertyAccessEntityNameExpression;
        };
    }
    interface JSDocAuthorTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocAuthorTag;
    }
    interface JSDocDeprecatedTag extends JSDocTag {
        kind: SyntaxKind.JSDocDeprecatedTag;
    }
    interface JSDocClassTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocClassTag;
    }
    interface JSDocPublicTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocPublicTag;
    }
    interface JSDocPrivateTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocPrivateTag;
    }
    interface JSDocProtectedTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocProtectedTag;
    }
    interface JSDocReadonlyTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocReadonlyTag;
    }
    interface JSDocOverrideTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocOverrideTag;
    }
    interface JSDocEnumTag extends JSDocTag, Declaration, LocalsContainer {
        readonly kind: SyntaxKind.JSDocEnumTag;
        readonly parent: JSDoc;
        readonly typeExpression: JSDocTypeExpression;
    }
    interface JSDocThisTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocThisTag;
        readonly typeExpression: JSDocTypeExpression;
    }
    interface JSDocTemplateTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocTemplateTag;
        readonly constraint: JSDocTypeExpression | undefined;
        readonly typeParameters: NodeArray<TypeParameterDeclaration>;
    }
    interface JSDocSeeTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocSeeTag;
        readonly name?: JSDocNameReference;
    }
    interface JSDocReturnTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocReturnTag;
        readonly typeExpression?: JSDocTypeExpression;
    }
    interface JSDocTypeTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocTypeTag;
        readonly typeExpression: JSDocTypeExpression;
    }
    interface JSDocTypedefTag extends JSDocTag, NamedDeclaration, LocalsContainer {
        readonly kind: SyntaxKind.JSDocTypedefTag;
        readonly parent: JSDoc;
        readonly fullName?: JSDocNamespaceDeclaration | Identifier;
        readonly name?: Identifier;
        readonly typeExpression?: JSDocTypeExpression | JSDocTypeLiteral;
    }
    interface JSDocCallbackTag extends JSDocTag, NamedDeclaration, LocalsContainer {
        readonly kind: SyntaxKind.JSDocCallbackTag;
        readonly parent: JSDoc;
        readonly fullName?: JSDocNamespaceDeclaration | Identifier;
        readonly name?: Identifier;
        readonly typeExpression: JSDocSignature;
    }
    interface JSDocOverloadTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocOverloadTag;
        readonly parent: JSDoc;
        readonly typeExpression: JSDocSignature;
    }
    interface JSDocThrowsTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocThrowsTag;
        readonly typeExpression?: JSDocTypeExpression;
    }
    interface JSDocSignature extends JSDocType, Declaration, JSDocContainer, LocalsContainer {
        readonly kind: SyntaxKind.JSDocSignature;
        readonly typeParameters?: readonly JSDocTemplateTag[];
        readonly parameters: readonly JSDocParameterTag[];
        readonly type: JSDocReturnTag | undefined;
    }
    interface JSDocPropertyLikeTag extends JSDocTag, Declaration {
        readonly parent: JSDoc;
        readonly name: EntityName;
        readonly typeExpression?: JSDocTypeExpression;
        readonly isNameFirst: boolean;
        readonly isBracketed: boolean;
    }
    interface JSDocPropertyTag extends JSDocPropertyLikeTag {
        readonly kind: SyntaxKind.JSDocPropertyTag;
    }
    interface JSDocParameterTag extends JSDocPropertyLikeTag {
        readonly kind: SyntaxKind.JSDocParameterTag;
    }
    interface JSDocTypeLiteral extends JSDocType, Declaration {
        readonly kind: SyntaxKind.JSDocTypeLiteral;
        readonly jsDocPropertyTags?: readonly JSDocPropertyLikeTag[];
        readonly isArrayType: boolean;
    }
    interface JSDocSatisfiesTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocSatisfiesTag;
        readonly typeExpression: JSDocTypeExpression;
    }
    interface JSDocImportTag extends JSDocTag {
        readonly kind: SyntaxKind.JSDocImportTag;
        readonly parent: JSDoc;
        readonly importClause?: ImportClause;
        readonly moduleSpecifier: Expression;
        readonly attributes?: ImportAttributes;
    }
    type FlowType = Type | IncompleteType;
    interface IncompleteType {
        flags: TypeFlags | 0;
        type: Type;
    }
    interface AmdDependency {
        path: string;
        name?: string;
    }
    interface SourceFileLike {
        readonly text: string;
    }
    interface SourceFileLike {
        getLineAndCharacterOfPosition(pos: number): LineAndCharacter;
    }
    type ResolutionMode = ModuleKind.ESNext | ModuleKind.CommonJS | undefined;
    interface SourceFile extends Declaration, LocalsContainer {
        readonly kind: SyntaxKind.SourceFile;
        readonly statements: NodeArray<Statement>;
        readonly endOfFileToken: Token<SyntaxKind.EndOfFileToken>;
        fileName: string;
        text: string;
        amdDependencies: readonly AmdDependency[];
        moduleName?: string;
        referencedFiles: readonly FileReference[];
        typeReferenceDirectives: readonly FileReference[];
        libReferenceDirectives: readonly FileReference[];
        languageVariant: LanguageVariant;
        isDeclarationFile: boolean;
        hasNoDefaultLib: boolean;
        languageVersion: ScriptTarget;
        impliedNodeFormat?: ResolutionMode;
    }
    interface SourceFile {
        getLineAndCharacterOfPosition(pos: number): LineAndCharacter;
        getLineEndOfPosition(pos: number): number;
        getLineStarts(): readonly number[];
        getPositionOfLineAndCharacter(line: number, character: number): number;
        update(newText: string, textChangeRange: TextChangeRange): SourceFile;
    }
    interface Bundle extends Node {
        readonly kind: SyntaxKind.Bundle;
        readonly sourceFiles: readonly SourceFile[];
    }
    interface JsonSourceFile extends SourceFile {
        readonly statements: NodeArray<JsonObjectExpressionStatement>;
    }
    interface TsConfigSourceFile extends JsonSourceFile {
        extendedSourceFiles?: string[];
    }
    interface JsonMinusNumericLiteral extends PrefixUnaryExpression {
        readonly kind: SyntaxKind.PrefixUnaryExpression;
        readonly operator: SyntaxKind.MinusToken;
        readonly operand: NumericLiteral;
    }
    type JsonObjectExpression = ObjectLiteralExpression | ArrayLiteralExpression | JsonMinusNumericLiteral | NumericLiteral | StringLiteral | BooleanLiteral | NullLiteral;
    interface JsonObjectExpressionStatement extends ExpressionStatement {
        readonly expression: JsonObjectExpression;
    }
    interface ScriptReferenceHost {
        getCompilerOptions(): CompilerOptions;
        getSourceFile(fileName: string): SourceFile | undefined;
        getSourceFileByPath(path: Path): SourceFile | undefined;
        getCurrentDirectory(): string;
    }
    interface ParseConfigHost extends ModuleResolutionHost {
        useCaseSensitiveFileNames: boolean;
        readDirectory(rootDir: string, extensions: readonly string[], excludes: readonly string[] | undefined, includes: readonly string[], depth?: number): readonly string[];
        fileExists(path: string): boolean;
        readFile(path: string): string | undefined;
        trace?(s: string): void;
    }
    type ResolvedConfigFileName = string & {
        _isResolvedConfigFileName: never;
    };
    interface WriteFileCallbackData {
    }
    type WriteFileCallback = (fileName: string, text: string, writeByteOrderMark: boolean, onError?: (message: string) => void, sourceFiles?: readonly SourceFile[], data?: WriteFileCallbackData) => void;
    class OperationCanceledException {
    }
    interface CancellationToken {
        isCancellationRequested(): boolean;
        throwIfCancellationRequested(): void;
    }
    interface Program extends ScriptReferenceHost {
        getCurrentDirectory(): string;
        getRootFileNames(): readonly string[];
        getSourceFiles(): readonly SourceFile[];
        emit(targetSourceFile?: SourceFile, writeFile?: WriteFileCallback, cancellationToken?: CancellationToken, emitOnlyDtsFiles?: boolean, customTransformers?: CustomTransformers): EmitResult;
        getOptionsDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getGlobalDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getSyntacticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly DiagnosticWithLocation[];
        getSemanticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
        getDeclarationDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly DiagnosticWithLocation[];
        getConfigFileParsingDiagnostics(): readonly Diagnostic[];
        getTypeChecker(): TypeChecker;
        getNodeCount(): number;
        getIdentifierCount(): number;
        getSymbolCount(): number;
        getTypeCount(): number;
        getInstantiationCount(): number;
        getRelationCacheSizes(): {
            assignable: number;
            identity: number;
            subtype: number;
            strictSubtype: number;
        };
        isSourceFileFromExternalLibrary(file: SourceFile): boolean;
        isSourceFileDefaultLibrary(file: SourceFile): boolean;
        getModeForUsageLocation(file: SourceFile, usage: StringLiteralLike): ResolutionMode;
        getModeForResolutionAtIndex(file: SourceFile, index: number): ResolutionMode;
        getProjectReferences(): readonly ProjectReference[] | undefined;
        getResolvedProjectReferences(): readonly (ResolvedProjectReference | undefined)[] | undefined;
    }
    interface ResolvedProjectReference {
        commandLine: ParsedCommandLine;
        sourceFile: SourceFile;
        references?: readonly (ResolvedProjectReference | undefined)[];
    }
    type CustomTransformerFactory = (context: TransformationContext) => CustomTransformer;
    interface CustomTransformer {
        transformSourceFile(node: SourceFile): SourceFile;
        transformBundle(node: Bundle): Bundle;
    }
    interface CustomTransformers {
        before?: (TransformerFactory<SourceFile> | CustomTransformerFactory)[];
        after?: (TransformerFactory<SourceFile> | CustomTransformerFactory)[];
        afterDeclarations?: (TransformerFactory<Bundle | SourceFile> | CustomTransformerFactory)[];
    }
    interface SourceMapSpan {
        emittedLine: number;
        emittedColumn: number;
        sourceLine: number;
        sourceColumn: number;
        nameIndex?: number;
        sourceIndex: number;
    }
    enum ExitStatus {
        Success = 0,
        DiagnosticsPresent_OutputsSkipped = 1,
        DiagnosticsPresent_OutputsGenerated = 2,
        InvalidProject_OutputsSkipped = 3,
        ProjectReferenceCycle_OutputsSkipped = 4,
    }
    interface EmitResult {
        emitSkipped: boolean;
        diagnostics: readonly Diagnostic[];
        emittedFiles?: string[];
    }
    interface TypeChecker {
        getTypeOfSymbolAtLocation(symbol: Symbol, node: Node): Type;
        getTypeOfSymbol(symbol: Symbol): Type;
        getDeclaredTypeOfSymbol(symbol: Symbol): Type;
        getPropertiesOfType(type: Type): Symbol[];
        getPropertyOfType(type: Type, propertyName: string): Symbol | undefined;
        getPrivateIdentifierPropertyOfType(leftType: Type, name: string, location: Node): Symbol | undefined;
        getIndexInfoOfType(type: Type, kind: IndexKind): IndexInfo | undefined;
        getIndexInfosOfType(type: Type): readonly IndexInfo[];
        getIndexInfosOfIndexSymbol: (indexSymbol: Symbol, siblingSymbols?: Symbol[] | undefined) => IndexInfo[];
        getSignaturesOfType(type: Type, kind: SignatureKind): readonly Signature[];
        getIndexTypeOfType(type: Type, kind: IndexKind): Type | undefined;
        getBaseTypes(type: InterfaceType): BaseType[];
        getBaseTypeOfLiteralType(type: Type): Type;
        getWidenedType(type: Type): Type;
        getAwaitedType(type: Type): Type | undefined;
        getReturnTypeOfSignature(signature: Signature): Type;
        getNullableType(type: Type, flags: TypeFlags): Type;
        getNonNullableType(type: Type): Type;
        getTypeArguments(type: TypeReference): readonly Type[];
        typeToTypeNode(type: Type, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): TypeNode | undefined;
        signatureToSignatureDeclaration(signature: Signature, kind: SyntaxKind, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined):
            | SignatureDeclaration & {
                typeArguments?: NodeArray<TypeNode>;
            }
            | undefined;
        indexInfoToIndexSignatureDeclaration(indexInfo: IndexInfo, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): IndexSignatureDeclaration | undefined;
        symbolToEntityName(symbol: Symbol, meaning: SymbolFlags, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): EntityName | undefined;
        symbolToExpression(symbol: Symbol, meaning: SymbolFlags, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): Expression | undefined;
        symbolToTypeParameterDeclarations(symbol: Symbol, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): NodeArray<TypeParameterDeclaration> | undefined;
        symbolToParameterDeclaration(symbol: Symbol, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): ParameterDeclaration | undefined;
        typeParameterToDeclaration(parameter: TypeParameter, enclosingDeclaration: Node | undefined, flags: NodeBuilderFlags | undefined): TypeParameterDeclaration | undefined;
        getSymbolsInScope(location: Node, meaning: SymbolFlags): Symbol[];
        getSymbolAtLocation(node: Node): Symbol | undefined;
        getSymbolsOfParameterPropertyDeclaration(parameter: ParameterDeclaration, parameterName: string): Symbol[];
        getShorthandAssignmentValueSymbol(location: Node | undefined): Symbol | undefined;
        getExportSpecifierLocalTargetSymbol(location: ExportSpecifier | Identifier): Symbol | undefined;
        getExportSymbolOfSymbol(symbol: Symbol): Symbol;
        getPropertySymbolOfDestructuringAssignment(location: Identifier): Symbol | undefined;
        getTypeOfAssignmentPattern(pattern: AssignmentPattern): Type;
        getTypeAtLocation(node: Node): Type;
        getTypeFromTypeNode(node: TypeNode): Type;
        signatureToString(signature: Signature, enclosingDeclaration?: Node, flags?: TypeFormatFlags, kind?: SignatureKind): string;
        typeToString(type: Type, enclosingDeclaration?: Node, flags?: TypeFormatFlags): string;
        symbolToString(symbol: Symbol, enclosingDeclaration?: Node, meaning?: SymbolFlags, flags?: SymbolFormatFlags): string;
        typePredicateToString(predicate: TypePredicate, enclosingDeclaration?: Node, flags?: TypeFormatFlags): string;
        getFullyQualifiedName(symbol: Symbol): string;
        getAugmentedPropertiesOfType(type: Type): Symbol[];
        getRootSymbols(symbol: Symbol): readonly Symbol[];
        getSymbolOfExpando(node: Node, allowDeclaration: boolean): Symbol | undefined;
        getContextualType(node: Expression): Type | undefined;
        getResolvedSignature(node: CallLikeExpression, candidatesOutArray?: Signature[], argumentCount?: number): Signature | undefined;
        getSignatureFromDeclaration(declaration: SignatureDeclaration): Signature | undefined;
        isImplementationOfOverload(node: SignatureDeclaration): boolean | undefined;
        isUndefinedSymbol(symbol: Symbol): boolean;
        isArgumentsSymbol(symbol: Symbol): boolean;
        isUnknownSymbol(symbol: Symbol): boolean;
        getMergedSymbol(symbol: Symbol): Symbol;
        getConstantValue(node: EnumMember | PropertyAccessExpression | ElementAccessExpression): string | number | undefined;
        isValidPropertyAccess(node: PropertyAccessExpression | QualifiedName | ImportTypeNode, propertyName: string): boolean;
        getAliasedSymbol(symbol: Symbol): Symbol;
        getImmediateAliasedSymbol(symbol: Symbol): Symbol | undefined;
        getExportsOfModule(moduleSymbol: Symbol): Symbol[];
        getJsxIntrinsicTagNamesAt(location: Node): Symbol[];
        isOptionalParameter(node: ParameterDeclaration): boolean;
        getAmbientModules(): Symbol[];
        tryGetMemberInModuleExports(memberName: string, moduleSymbol: Symbol): Symbol | undefined;
        getApparentType(type: Type): Type;
        getBaseConstraintOfType(type: Type): Type | undefined;
        getDefaultFromTypeParameter(type: Type): Type | undefined;
        getAnyType(): Type;
        getStringType(): Type;
        getStringLiteralType(value: string): StringLiteralType;
        getNumberType(): Type;
        getNumberLiteralType(value: number): NumberLiteralType;
        getBigIntType(): Type;
        getBigIntLiteralType(value: PseudoBigInt): BigIntLiteralType;
        getBooleanType(): Type;
        getUnknownType(): Type;
        getFalseType(): Type;
        getTrueType(): Type;
        getVoidType(): Type;
        getUndefinedType(): Type;
        getNullType(): Type;
        getESSymbolType(): Type;
        getNeverType(): Type;
        getNonPrimitiveType(): Type;
        isTypeAssignableTo(source: Type, target: Type): boolean;
        isArrayType(type: Type): boolean;
        isTupleType(type: Type): boolean;
        isArrayLikeType(type: Type): boolean;
        resolveName(name: string, location: Node | undefined, meaning: SymbolFlags, excludeGlobals: boolean): Symbol | undefined;
        getTypePredicateOfSignature(signature: Signature): TypePredicate | undefined;
        runWithCancellationToken<T>(token: CancellationToken, cb: (checker: TypeChecker) => T): T;
        getTypeArgumentsForResolvedSignature(signature: Signature): readonly Type[] | undefined;
    }
    enum NodeBuilderFlags {
        None = 0,
        NoTruncation = 1,
        WriteArrayAsGenericType = 2,
        GenerateNamesForShadowedTypeParams = 4,
        UseStructuralFallback = 8,
        ForbidIndexedAccessSymbolReferences = 16,
        WriteTypeArgumentsOfSignature = 32,
        UseFullyQualifiedType = 64,
        UseOnlyExternalAliasing = 128,
        SuppressAnyReturnType = 256,
        WriteTypeParametersInQualifiedName = 512,
        MultilineObjectLiterals = 1024,
        WriteClassExpressionAsTypeLiteral = 2048,
        UseTypeOfFunction = 4096,
        OmitParameterModifiers = 8192,
        UseAliasDefinedOutsideCurrentScope = 16384,
        UseSingleQuotesForStringLiteralType = 268435456,
        NoTypeReduction = 536870912,
        OmitThisParameter = 33554432,
        AllowThisInObjectLiteral = 32768,
        AllowQualifiedNameInPlaceOfIdentifier = 65536,
        AllowAnonymousIdentifier = 131072,
        AllowEmptyUnionOrIntersection = 262144,
        AllowEmptyTuple = 524288,
        AllowUniqueESSymbolType = 1048576,
        AllowEmptyIndexInfoType = 2097152,
        AllowNodeModulesRelativePaths = 67108864,
        IgnoreErrors = 70221824,
        InObjectTypeLiteral = 4194304,
        InTypeAlias = 8388608,
        InInitialEntityName = 16777216,
    }
    enum TypeFormatFlags {
        None = 0,
        NoTruncation = 1,
        WriteArrayAsGenericType = 2,
        GenerateNamesForShadowedTypeParams = 4,
        UseStructuralFallback = 8,
        WriteTypeArgumentsOfSignature = 32,
        UseFullyQualifiedType = 64,
        SuppressAnyReturnType = 256,
        MultilineObjectLiterals = 1024,
        WriteClassExpressionAsTypeLiteral = 2048,
        UseTypeOfFunction = 4096,
        OmitParameterModifiers = 8192,
        UseAliasDefinedOutsideCurrentScope = 16384,
        UseSingleQuotesForStringLiteralType = 268435456,
        NoTypeReduction = 536870912,
        OmitThisParameter = 33554432,
        AllowUniqueESSymbolType = 1048576,
        AddUndefined = 131072,
        WriteArrowStyleSignature = 262144,
        InArrayType = 524288,
        InElementType = 2097152,
        InFirstTypeArgument = 4194304,
        InTypeAlias = 8388608,
        NodeBuilderFlagsMask = 848330095,
    }
    enum SymbolFormatFlags {
        None = 0,
        WriteTypeParametersOrArguments = 1,
        UseOnlyExternalAliasing = 2,
        AllowAnyNodeKind = 4,
        UseAliasDefinedOutsideCurrentScope = 8,
    }
    enum TypePredicateKind {
        This = 0,
        Identifier = 1,
        AssertsThis = 2,
        AssertsIdentifier = 3,
    }
    interface TypePredicateBase {
        kind: TypePredicateKind;
        type: Type | undefined;
    }
    interface ThisTypePredicate extends TypePredicateBase {
        kind: TypePredicateKind.This;
        parameterName: undefined;
        parameterIndex: undefined;
        type: Type;
    }
    interface IdentifierTypePredicate extends TypePredicateBase {
        kind: TypePredicateKind.Identifier;
        parameterName: string;
        parameterIndex: number;
        type: Type;
    }
    interface AssertsThisTypePredicate extends TypePredicateBase {
        kind: TypePredicateKind.AssertsThis;
        parameterName: undefined;
        parameterIndex: undefined;
        type: Type | undefined;
    }
    interface AssertsIdentifierTypePredicate extends TypePredicateBase {
        kind: TypePredicateKind.AssertsIdentifier;
        parameterName: string;
        parameterIndex: number;
        type: Type | undefined;
    }
    type TypePredicate = ThisTypePredicate | IdentifierTypePredicate | AssertsThisTypePredicate | AssertsIdentifierTypePredicate;
    enum SymbolFlags {
        None = 0,
        FunctionScopedVariable = 1,
        BlockScopedVariable = 2,
        Property = 4,
        EnumMember = 8,
        Function = 16,
        Class = 32,
        Interface = 64,
        ConstEnum = 128,
        RegularEnum = 256,
        ValueModule = 512,
        NamespaceModule = 1024,
        TypeLiteral = 2048,
        ObjectLiteral = 4096,
        Method = 8192,
        Constructor = 16384,
        GetAccessor = 32768,
        SetAccessor = 65536,
        Signature = 131072,
        TypeParameter = 262144,
        TypeAlias = 524288,
        ExportValue = 1048576,
        Alias = 2097152,
        Prototype = 4194304,
        ExportStar = 8388608,
        Optional = 16777216,
        Transient = 33554432,
        Assignment = 67108864,
        ModuleExports = 134217728,
        All = -1,
        Enum = 384,
        Variable = 3,
        Value = 111551,
        Type = 788968,
        Namespace = 1920,
        Module = 1536,
        Accessor = 98304,
        FunctionScopedVariableExcludes = 111550,
        BlockScopedVariableExcludes = 111551,
        ParameterExcludes = 111551,
        PropertyExcludes = 0,
        EnumMemberExcludes = 900095,
        FunctionExcludes = 110991,
        ClassExcludes = 899503,
        InterfaceExcludes = 788872,
        RegularEnumExcludes = 899327,
        ConstEnumExcludes = 899967,
        ValueModuleExcludes = 110735,
        NamespaceModuleExcludes = 0,
        MethodExcludes = 103359,
        GetAccessorExcludes = 46015,
        SetAccessorExcludes = 78783,
        AccessorExcludes = 13247,
        TypeParameterExcludes = 526824,
        TypeAliasExcludes = 788968,
        AliasExcludes = 2097152,
        ModuleMember = 2623475,
        ExportHasLocal = 944,
        BlockScoped = 418,
        PropertyOrAccessor = 98308,
        ClassMember = 106500,
    }
    interface Symbol {
        flags: SymbolFlags;
        escapedName: __String;
        declarations?: Declaration[];
        valueDeclaration?: Declaration;
        members?: SymbolTable;
        exports?: SymbolTable;
        globalExports?: SymbolTable;
    }
    interface Symbol {
        readonly name: string;
        getFlags(): SymbolFlags;
        getEscapedName(): __String;
        getName(): string;
        getDeclarations(): Declaration[] | undefined;
        getDocumentationComment(typeChecker: TypeChecker | undefined): SymbolDisplayPart[];
        getJsDocTags(checker?: TypeChecker): JSDocTagInfo[];
    }
    enum InternalSymbolName {
        Call = "__call",
        Constructor = "__constructor",
        New = "__new",
        Index = "__index",
        ExportStar = "__export",
        Global = "__global",
        Missing = "__missing",
        Type = "__type",
        Object = "__object",
        JSXAttributes = "__jsxAttributes",
        Class = "__class",
        Function = "__function",
        Computed = "__computed",
        Resolving = "__resolving__",
        ExportEquals = "export=",
        Default = "default",
        This = "this",
        InstantiationExpression = "__instantiationExpression",
        ImportAttributes = "__importAttributes",
    }
    type __String =
        | (string & {
            __escapedIdentifier: void;
        })
        | (void & {
            __escapedIdentifier: void;
        })
        | InternalSymbolName;
    type ReadonlyUnderscoreEscapedMap<T> = ReadonlyMap<__String, T>;
    type UnderscoreEscapedMap<T> = Map<__String, T>;
    type SymbolTable = Map<__String, Symbol>;
    enum TypeFlags {
        Any = 1,
        Unknown = 2,
        String = 4,
        Number = 8,
        Boolean = 16,
        Enum = 32,
        BigInt = 64,
        StringLiteral = 128,
        NumberLiteral = 256,
        BooleanLiteral = 512,
        EnumLiteral = 1024,
        BigIntLiteral = 2048,
        ESSymbol = 4096,
        UniqueESSymbol = 8192,
        Void = 16384,
        Undefined = 32768,
        Null = 65536,
        Never = 131072,
        TypeParameter = 262144,
        Object = 524288,
        Union = 1048576,
        Intersection = 2097152,
        Index = 4194304,
        IndexedAccess = 8388608,
        Conditional = 16777216,
        Substitution = 33554432,
        NonPrimitive = 67108864,
        TemplateLiteral = 134217728,
        StringMapping = 268435456,
        Literal = 2944,
        Unit = 109472,
        Freshable = 2976,
        StringOrNumberLiteral = 384,
        PossiblyFalsy = 117724,
        StringLike = 402653316,
        NumberLike = 296,
        BigIntLike = 2112,
        BooleanLike = 528,
        EnumLike = 1056,
        ESSymbolLike = 12288,
        VoidLike = 49152,
        UnionOrIntersection = 3145728,
        StructuredType = 3670016,
        TypeVariable = 8650752,
        InstantiableNonPrimitive = 58982400,
        InstantiablePrimitive = 406847488,
        Instantiable = 465829888,
        StructuredOrInstantiable = 469499904,
        Narrowable = 536624127,
    }
    type DestructuringPattern = BindingPattern | ObjectLiteralExpression | ArrayLiteralExpression;
    interface Type {
        flags: TypeFlags;
        symbol: Symbol;
        pattern?: DestructuringPattern;
        aliasSymbol?: Symbol;
        aliasTypeArguments?: readonly Type[];
    }
    interface Type {
        getFlags(): TypeFlags;
        getSymbol(): Symbol | undefined;
        getProperties(): Symbol[];
        getProperty(propertyName: string): Symbol | undefined;
        getApparentProperties(): Symbol[];
        getCallSignatures(): readonly Signature[];
        getConstructSignatures(): readonly Signature[];
        getStringIndexType(): Type | undefined;
        getNumberIndexType(): Type | undefined;
        getBaseTypes(): BaseType[] | undefined;
        getNonNullableType(): Type;
        getConstraint(): Type | undefined;
        getDefault(): Type | undefined;
        isUnion(): this is UnionType;
        isIntersection(): this is IntersectionType;
        isUnionOrIntersection(): this is UnionOrIntersectionType;
        isLiteral(): this is LiteralType;
        isStringLiteral(): this is StringLiteralType;
        isNumberLiteral(): this is NumberLiteralType;
        isTypeParameter(): this is TypeParameter;
        isClassOrInterface(): this is InterfaceType;
        isClass(): this is InterfaceType;
        isIndexType(): this is IndexType;
    }
    interface FreshableType extends Type {
        freshType: FreshableType;
        regularType: FreshableType;
    }
    interface LiteralType extends FreshableType {
        value: string | number | PseudoBigInt;
    }
    interface UniqueESSymbolType extends Type {
        symbol: Symbol;
        escapedName: __String;
    }
    interface StringLiteralType extends LiteralType {
        value: string;
    }
    interface NumberLiteralType extends LiteralType {
        value: number;
    }
    interface BigIntLiteralType extends LiteralType {
        value: PseudoBigInt;
    }
    interface EnumType extends FreshableType {
    }
    enum ObjectFlags {
        None = 0,
        Class = 1,
        Interface = 2,
        Reference = 4,
        Tuple = 8,
        Anonymous = 16,
        Mapped = 32,
        Instantiated = 64,
        ObjectLiteral = 128,
        EvolvingArray = 256,
        ObjectLiteralPatternWithComputedProperties = 512,
        ReverseMapped = 1024,
        JsxAttributes = 2048,
        JSLiteral = 4096,
        FreshLiteral = 8192,
        ArrayLiteral = 16384,
        SingleSignatureType = 134217728,
        ClassOrInterface = 3,
        ContainsSpread = 2097152,
        ObjectRestType = 4194304,
        InstantiationExpressionType = 8388608,
    }
    interface ObjectType extends Type {
        objectFlags: ObjectFlags;
    }
    interface InterfaceType extends ObjectType {
        typeParameters: TypeParameter[] | undefined;
        outerTypeParameters: TypeParameter[] | undefined;
        localTypeParameters: TypeParameter[] | undefined;
        thisType: TypeParameter | undefined;
    }
    type BaseType = ObjectType | IntersectionType | TypeVariable;
    interface InterfaceTypeWithDeclaredMembers extends InterfaceType {
        declaredProperties: Symbol[];
        declaredCallSignatures: Signature[];
        declaredConstructSignatures: Signature[];
        declaredIndexInfos: IndexInfo[];
    }
    interface TypeReference extends ObjectType {
        target: GenericType;
        node?: TypeReferenceNode | ArrayTypeNode | TupleTypeNode;
    }
    interface TypeReference {
        typeArguments?: readonly Type[];
    }
    interface DeferredTypeReference extends TypeReference {
    }
    interface GenericType extends InterfaceType, TypeReference {
    }
    enum ElementFlags {
        Required = 1,
        Optional = 2,
        Rest = 4,
        Variadic = 8,
        Fixed = 3,
        Variable = 12,
        NonRequired = 14,
        NonRest = 11,
    }
    interface TupleType extends GenericType {
        elementFlags: readonly ElementFlags[];
        minLength: number;
        fixedLength: number;
        hasRestElement: boolean;
        combinedFlags: ElementFlags;
        readonly: boolean;
        labeledElementDeclarations?: readonly (NamedTupleMember | ParameterDeclaration | undefined)[];
    }
    interface TupleTypeReference extends TypeReference {
        target: TupleType;
    }
    interface UnionOrIntersectionType extends Type {
        types: Type[];
    }
    interface UnionType extends UnionOrIntersectionType {
    }
    interface IntersectionType extends UnionOrIntersectionType {
    }
    type StructuredType = ObjectType | UnionType | IntersectionType;
    interface EvolvingArrayType extends ObjectType {
        elementType: Type;
        finalArrayType?: Type;
    }
    interface InstantiableType extends Type {
    }
    interface TypeParameter extends InstantiableType {
    }
    interface IndexedAccessType extends InstantiableType {
        objectType: Type;
        indexType: Type;
        constraint?: Type;
        simplifiedForReading?: Type;
        simplifiedForWriting?: Type;
    }
    type TypeVariable = TypeParameter | IndexedAccessType;
    interface IndexType extends InstantiableType {
        type: InstantiableType | UnionOrIntersectionType;
    }
    interface ConditionalRoot {
        node: ConditionalTypeNode;
        checkType: Type;
        extendsType: Type;
        isDistributive: boolean;
        inferTypeParameters?: TypeParameter[];
        outerTypeParameters?: TypeParameter[];
        instantiations?: Map<string, Type>;
        aliasSymbol?: Symbol;
        aliasTypeArguments?: Type[];
    }
    interface ConditionalType extends InstantiableType {
        root: ConditionalRoot;
        checkType: Type;
        extendsType: Type;
        resolvedTrueType?: Type;
        resolvedFalseType?: Type;
    }
    interface TemplateLiteralType extends InstantiableType {
        texts: readonly string[];
        types: readonly Type[];
    }
    interface StringMappingType extends InstantiableType {
        symbol: Symbol;
        type: Type;
    }
    interface SubstitutionType extends InstantiableType {
        objectFlags: ObjectFlags;
        baseType: Type;
        constraint: Type;
    }
    enum SignatureKind {
        Call = 0,
        Construct = 1,
    }
    interface Signature {
        declaration?: SignatureDeclaration | JSDocSignature;
        typeParameters?: readonly TypeParameter[];
        parameters: readonly Symbol[];
        thisParameter?: Symbol;
    }
    interface Signature {
        getDeclaration(): SignatureDeclaration;
        getTypeParameters(): TypeParameter[] | undefined;
        getParameters(): Symbol[];
        getTypeParameterAtPosition(pos: number): Type;
        getReturnType(): Type;
        getDocumentationComment(typeChecker: TypeChecker | undefined): SymbolDisplayPart[];
        getJsDocTags(): JSDocTagInfo[];
    }
    enum IndexKind {
        String = 0,
        Number = 1,
    }
    type ElementWithComputedPropertyName = (ClassElement | ObjectLiteralElement) & {
        name: ComputedPropertyName;
    };
    interface IndexInfo {
        keyType: Type;
        type: Type;
        isReadonly: boolean;
        declaration?: IndexSignatureDeclaration;
        components?: ElementWithComputedPropertyName[];
    }
    enum InferencePriority {
        None = 0,
        NakedTypeVariable = 1,
        SpeculativeTuple = 2,
        SubstituteSource = 4,
        HomomorphicMappedType = 8,
        PartialHomomorphicMappedType = 16,
        MappedTypeConstraint = 32,
        ContravariantConditional = 64,
        ReturnType = 128,
        LiteralKeyof = 256,
        NoConstraints = 512,
        AlwaysStrict = 1024,
        MaxValue = 2048,
        PriorityImpliesCombination = 416,
        Circularity = -1,
    }
    interface FileExtensionInfo {
        extension: string;
        isMixedContent: boolean;
        scriptKind?: ScriptKind;
    }
    interface DiagnosticMessage {
        key: string;
        category: DiagnosticCategory;
        code: number;
        message: string;
        reportsUnnecessary?: {};
        reportsDeprecated?: {};
    }
    interface DiagnosticMessageChain {
        messageText: string;
        category: DiagnosticCategory;
        code: number;
        next?: DiagnosticMessageChain[];
    }
    interface Diagnostic extends DiagnosticRelatedInformation {
        reportsUnnecessary?: {};
        reportsDeprecated?: {};
        source?: string;
        relatedInformation?: DiagnosticRelatedInformation[];
    }
    interface DiagnosticRelatedInformation {
        category: DiagnosticCategory;
        code: number;
        file: SourceFile | undefined;
        start: number | undefined;
        length: number | undefined;
        messageText: string | DiagnosticMessageChain;
    }
    interface DiagnosticWithLocation extends Diagnostic {
        file: SourceFile;
        start: number;
        length: number;
    }
    enum DiagnosticCategory {
        Warning = 0,
        Error = 1,
        Suggestion = 2,
        Message = 3,
    }
    enum ModuleResolutionKind {
        Classic = 1,
        NodeJs = 2,
        Node10 = 2,
        Node16 = 3,
        NodeNext = 99,
        Bundler = 100,
    }
    enum ModuleDetectionKind {
        Legacy = 1,
        Auto = 2,
        Force = 3,
    }
    interface PluginImport {
        name: string;
    }
    interface ProjectReference {
        path: string;
        originalPath?: string;
        prepend?: boolean;
        circular?: boolean;
    }
    enum WatchFileKind {
        FixedPollingInterval = 0,
        PriorityPollingInterval = 1,
        DynamicPriorityPolling = 2,
        FixedChunkSizePolling = 3,
        UseFsEvents = 4,
        UseFsEventsOnParentDirectory = 5,
    }
    enum WatchDirectoryKind {
        UseFsEvents = 0,
        FixedPollingInterval = 1,
        DynamicPriorityPolling = 2,
        FixedChunkSizePolling = 3,
    }
    enum PollingWatchKind {
        FixedInterval = 0,
        PriorityInterval = 1,
        DynamicPriority = 2,
        FixedChunkSize = 3,
    }
    type CompilerOptionsValue = string | number | boolean | (string | number)[] | string[] | MapLike<string[]> | PluginImport[] | ProjectReference[] | null | undefined;
    interface CompilerOptions {
        allowImportingTsExtensions?: boolean;
        allowJs?: boolean;
        allowArbitraryExtensions?: boolean;
        allowSyntheticDefaultImports?: boolean;
        allowUmdGlobalAccess?: boolean;
        allowUnreachableCode?: boolean;
        allowUnusedLabels?: boolean;
        alwaysStrict?: boolean;
        baseUrl?: string;
        charset?: string;
        checkJs?: boolean;
        customConditions?: string[];
        declaration?: boolean;
        declarationMap?: boolean;
        emitDeclarationOnly?: boolean;
        declarationDir?: string;
        disableSizeLimit?: boolean;
        disableSourceOfProjectReferenceRedirect?: boolean;
        disableSolutionSearching?: boolean;
        disableReferencedProjectLoad?: boolean;
        downlevelIteration?: boolean;
        emitBOM?: boolean;
        emitDecoratorMetadata?: boolean;
        exactOptionalPropertyTypes?: boolean;
        experimentalDecorators?: boolean;
        forceConsistentCasingInFileNames?: boolean;
        ignoreDeprecations?: string;
        importHelpers?: boolean;
        importsNotUsedAsValues?: ImportsNotUsedAsValues;
        inlineSourceMap?: boolean;
        inlineSources?: boolean;
        isolatedModules?: boolean;
        isolatedDeclarations?: boolean;
        jsx?: JsxEmit;
        keyofStringsOnly?: boolean;
        lib?: string[];
        libReplacement?: boolean;
        locale?: string;
        mapRoot?: string;
        maxNodeModuleJsDepth?: number;
        module?: ModuleKind;
        moduleResolution?: ModuleResolutionKind;
        moduleSuffixes?: string[];
        moduleDetection?: ModuleDetectionKind;
        newLine?: NewLineKind;
        noEmit?: boolean;
        noCheck?: boolean;
        noEmitHelpers?: boolean;
        noEmitOnError?: boolean;
        noErrorTruncation?: boolean;
        noFallthroughCasesInSwitch?: boolean;
        noImplicitAny?: boolean;
        noImplicitReturns?: boolean;
        noImplicitThis?: boolean;
        noStrictGenericChecks?: boolean;
        noUnusedLocals?: boolean;
        noUnusedParameters?: boolean;
        noImplicitUseStrict?: boolean;
        noPropertyAccessFromIndexSignature?: boolean;
        assumeChangesOnlyAffectDirectDependencies?: boolean;
        noLib?: boolean;
        noResolve?: boolean;
        noUncheckedIndexedAccess?: boolean;
        out?: string;
        outDir?: string;
        outFile?: string;
        paths?: MapLike<string[]>;
        preserveConstEnums?: boolean;
        noImplicitOverride?: boolean;
        preserveSymlinks?: boolean;
        preserveValueImports?: boolean;
        project?: string;
        reactNamespace?: string;
        jsxFactory?: string;
        jsxFragmentFactory?: string;
        jsxImportSource?: string;
        composite?: boolean;
        incremental?: boolean;
        tsBuildInfoFile?: string;
        removeComments?: boolean;
        resolvePackageJsonExports?: boolean;
        resolvePackageJsonImports?: boolean;
        rewriteRelativeImportExtensions?: boolean;
        rootDir?: string;
        rootDirs?: string[];
        skipLibCheck?: boolean;
        skipDefaultLibCheck?: boolean;
        sourceMap?: boolean;
        sourceRoot?: string;
        strict?: boolean;
        strictFunctionTypes?: boolean;
        strictBindCallApply?: boolean;
        strictNullChecks?: boolean;
        strictPropertyInitialization?: boolean;
        strictBuiltinIteratorReturn?: boolean;
        stripInternal?: boolean;
        suppressExcessPropertyErrors?: boolean;
        suppressImplicitAnyIndexErrors?: boolean;
        target?: ScriptTarget;
        traceResolution?: boolean;
        useUnknownInCatchVariables?: boolean;
        noUncheckedSideEffectImports?: boolean;
        resolveJsonModule?: boolean;
        types?: string[];
        typeRoots?: string[];
        verbatimModuleSyntax?: boolean;
        erasableSyntaxOnly?: boolean;
        esModuleInterop?: boolean;
        useDefineForClassFields?: boolean;
        [option: string]: CompilerOptionsValue | TsConfigSourceFile | undefined;
    }
    interface WatchOptions {
        watchFile?: WatchFileKind;
        watchDirectory?: WatchDirectoryKind;
        fallbackPolling?: PollingWatchKind;
        synchronousWatchDirectory?: boolean;
        excludeDirectories?: string[];
        excludeFiles?: string[];
        [option: string]: CompilerOptionsValue | undefined;
    }
    interface TypeAcquisition {
        enable?: boolean;
        include?: string[];
        exclude?: string[];
        disableFilenameBasedTypeAcquisition?: boolean;
        [option: string]: CompilerOptionsValue | undefined;
    }
    enum ModuleKind {
        None = 0,
        CommonJS = 1,
        AMD = 2,
        UMD = 3,
        System = 4,
        ES2015 = 5,
        ES2020 = 6,
        ES2022 = 7,
        ESNext = 99,
        Node16 = 100,
        Node18 = 101,
        Node20 = 102,
        NodeNext = 199,
        Preserve = 200,
    }
    enum JsxEmit {
        None = 0,
        Preserve = 1,
        React = 2,
        ReactNative = 3,
        ReactJSX = 4,
        ReactJSXDev = 5,
    }
    enum ImportsNotUsedAsValues {
        Remove = 0,
        Preserve = 1,
        Error = 2,
    }
    enum NewLineKind {
        CarriageReturnLineFeed = 0,
        LineFeed = 1,
    }
    interface LineAndCharacter {
        line: number;
        character: number;
    }
    enum ScriptKind {
        Unknown = 0,
        JS = 1,
        JSX = 2,
        TS = 3,
        TSX = 4,
        External = 5,
        JSON = 6,
        Deferred = 7,
    }
    enum ScriptTarget {
        ES3 = 0,
        ES5 = 1,
        ES2015 = 2,
        ES2016 = 3,
        ES2017 = 4,
        ES2018 = 5,
        ES2019 = 6,
        ES2020 = 7,
        ES2021 = 8,
        ES2022 = 9,
        ES2023 = 10,
        ES2024 = 11,
        ESNext = 99,
        JSON = 100,
        Latest = 99,
    }
    enum LanguageVariant {
        Standard = 0,
        JSX = 1,
    }
    interface ParsedCommandLine {
        options: CompilerOptions;
        typeAcquisition?: TypeAcquisition;
        fileNames: string[];
        projectReferences?: readonly ProjectReference[];
        watchOptions?: WatchOptions;
        raw?: any;
        errors: Diagnostic[];
        wildcardDirectories?: MapLike<WatchDirectoryFlags>;
        compileOnSave?: boolean;
    }
    enum WatchDirectoryFlags {
        None = 0,
        Recursive = 1,
    }
    interface CreateProgramOptions {
        rootNames: readonly string[];
        options: CompilerOptions;
        projectReferences?: readonly ProjectReference[];
        host?: CompilerHost;
        oldProgram?: Program;
        configFileParsingDiagnostics?: readonly Diagnostic[];
    }
    interface ModuleResolutionHost {
        fileExists(fileName: string): boolean;
        readFile(fileName: string): string | undefined;
        trace?(s: string): void;
        directoryExists?(directoryName: string): boolean;
        realpath?(path: string): string;
        getCurrentDirectory?(): string;
        getDirectories?(path: string): string[];
        useCaseSensitiveFileNames?: boolean | (() => boolean) | undefined;
    }
    interface MinimalResolutionCacheHost extends ModuleResolutionHost {
        getCompilationSettings(): CompilerOptions;
        getCompilerHost?(): CompilerHost | undefined;
    }
    interface ResolvedModule {
        resolvedFileName: string;
        isExternalLibraryImport?: boolean;
        resolvedUsingTsExtension?: boolean;
    }
    interface ResolvedModuleFull extends ResolvedModule {
        extension: string;
        packageId?: PackageId;
    }
    interface PackageId {
        name: string;
        subModuleName: string;
        version: string;
    }
    enum Extension {
        Ts = ".ts",
        Tsx = ".tsx",
        Dts = ".d.ts",
        Js = ".js",
        Jsx = ".jsx",
        Json = ".json",
        TsBuildInfo = ".tsbuildinfo",
        Mjs = ".mjs",
        Mts = ".mts",
        Dmts = ".d.mts",
        Cjs = ".cjs",
        Cts = ".cts",
        Dcts = ".d.cts",
    }
    interface ResolvedModuleWithFailedLookupLocations {
        readonly resolvedModule: ResolvedModuleFull | undefined;
    }
    interface ResolvedTypeReferenceDirective {
        primary: boolean;
        resolvedFileName: string | undefined;
        packageId?: PackageId;
        isExternalLibraryImport?: boolean;
    }
    interface ResolvedTypeReferenceDirectiveWithFailedLookupLocations {
        readonly resolvedTypeReferenceDirective: ResolvedTypeReferenceDirective | undefined;
    }
    interface CompilerHost extends ModuleResolutionHost {
        getSourceFile(fileName: string, languageVersionOrOptions: ScriptTarget | CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): SourceFile | undefined;
        getSourceFileByPath?(fileName: string, path: Path, languageVersionOrOptions: ScriptTarget | CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): SourceFile | undefined;
        getCancellationToken?(): CancellationToken;
        getDefaultLibFileName(options: CompilerOptions): string;
        getDefaultLibLocation?(): string;
        writeFile: WriteFileCallback;
        getCurrentDirectory(): string;
        getCanonicalFileName(fileName: string): string;
        useCaseSensitiveFileNames(): boolean;
        getNewLine(): string;
        readDirectory?(rootDir: string, extensions: readonly string[], excludes: readonly string[] | undefined, includes: readonly string[], depth?: number): string[];
        resolveModuleNames?(moduleNames: string[], containingFile: string, reusedNames: string[] | undefined, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile?: SourceFile): (ResolvedModule | undefined)[];
        getModuleResolutionCache?(): ModuleResolutionCache | undefined;
        resolveTypeReferenceDirectives?(typeReferenceDirectiveNames: string[] | readonly FileReference[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingFileMode?: ResolutionMode): (ResolvedTypeReferenceDirective | undefined)[];
        resolveModuleNameLiterals?(moduleLiterals: readonly StringLiteralLike[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile, reusedNames: readonly StringLiteralLike[] | undefined): readonly ResolvedModuleWithFailedLookupLocations[];
        resolveTypeReferenceDirectiveReferences?<T extends FileReference | string>(typeDirectiveReferences: readonly T[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile | undefined, reusedNames: readonly T[] | undefined): readonly ResolvedTypeReferenceDirectiveWithFailedLookupLocations[];
        getEnvironmentVariable?(name: string): string | undefined;
        hasInvalidatedResolutions?(filePath: Path): boolean;
        createHash?(data: string): string;
        getParsedCommandLine?(fileName: string): ParsedCommandLine | undefined;
        jsDocParsingMode?: JSDocParsingMode;
    }
    interface SourceMapRange extends TextRange {
        source?: SourceMapSource;
    }
    interface SourceMapSource {
        fileName: string;
        text: string;
        skipTrivia?: (pos: number) => number;
    }
    interface SourceMapSource {
        getLineAndCharacterOfPosition(pos: number): LineAndCharacter;
    }
    enum EmitFlags {
        None = 0,
        SingleLine = 1,
        MultiLine = 2,
        AdviseOnEmitNode = 4,
        NoSubstitution = 8,
        CapturesThis = 16,
        NoLeadingSourceMap = 32,
        NoTrailingSourceMap = 64,
        NoSourceMap = 96,
        NoNestedSourceMaps = 128,
        NoTokenLeadingSourceMaps = 256,
        NoTokenTrailingSourceMaps = 512,
        NoTokenSourceMaps = 768,
        NoLeadingComments = 1024,
        NoTrailingComments = 2048,
        NoComments = 3072,
        NoNestedComments = 4096,
        HelperName = 8192,
        ExportName = 16384,
        LocalName = 32768,
        InternalName = 65536,
        Indented = 131072,
        NoIndentation = 262144,
        AsyncFunctionBody = 524288,
        ReuseTempVariableScope = 1048576,
        CustomPrologue = 2097152,
        NoHoisting = 4194304,
        Iterator = 8388608,
        NoAsciiEscaping = 16777216,
    }
    interface EmitHelperBase {
        readonly name: string;
        readonly scoped: boolean;
        readonly text: string | ((node: EmitHelperUniqueNameCallback) => string);
        readonly priority?: number;
        readonly dependencies?: EmitHelper[];
    }
    interface ScopedEmitHelper extends EmitHelperBase {
        readonly scoped: true;
    }
    interface UnscopedEmitHelper extends EmitHelperBase {
        readonly scoped: false;
        readonly text: string;
    }
    type EmitHelper = ScopedEmitHelper | UnscopedEmitHelper;
    type EmitHelperUniqueNameCallback = (name: string) => string;
    enum EmitHint {
        SourceFile = 0,
        Expression = 1,
        IdentifierName = 2,
        MappedTypeParameter = 3,
        Unspecified = 4,
        EmbeddedStatement = 5,
        JsxAttributeValue = 6,
        ImportTypeNodeAttributes = 7,
    }
    enum OuterExpressionKinds {
        Parentheses = 1,
        TypeAssertions = 2,
        NonNullAssertions = 4,
        PartiallyEmittedExpressions = 8,
        ExpressionsWithTypeArguments = 16,
        Satisfies = 32,
        Assertions = 38,
        All = 63,
        ExcludeJSDocTypeAssertion = -2147483648,
    }
    type ImmediatelyInvokedFunctionExpression = CallExpression & {
        readonly expression: FunctionExpression;
    };
    type ImmediatelyInvokedArrowFunction = CallExpression & {
        readonly expression: ParenthesizedExpression & {
            readonly expression: ArrowFunction;
        };
    };
    interface NodeFactory {
        createNodeArray<T extends Node>(elements?: readonly T[], hasTrailingComma?: boolean): NodeArray<T>;
        createNumericLiteral(value: string | number, numericLiteralFlags?: TokenFlags): NumericLiteral;
        createBigIntLiteral(value: string | PseudoBigInt): BigIntLiteral;
        createStringLiteral(text: string, isSingleQuote?: boolean): StringLiteral;
        createStringLiteralFromNode(sourceNode: PropertyNameLiteral | PrivateIdentifier, isSingleQuote?: boolean): StringLiteral;
        createRegularExpressionLiteral(text: string): RegularExpressionLiteral;
        createIdentifier(text: string): Identifier;
        createTempVariable(recordTempVariable: ((node: Identifier) => void) | undefined, reservedInNestedScopes?: boolean): Identifier;
        createLoopVariable(reservedInNestedScopes?: boolean): Identifier;
        createUniqueName(text: string, flags?: GeneratedIdentifierFlags): Identifier;
        getGeneratedNameForNode(node: Node | undefined, flags?: GeneratedIdentifierFlags): Identifier;
        createPrivateIdentifier(text: string): PrivateIdentifier;
        createUniquePrivateName(text?: string): PrivateIdentifier;
        getGeneratedPrivateNameForNode(node: Node): PrivateIdentifier;
        createToken(token: SyntaxKind.SuperKeyword): SuperExpression;
        createToken(token: SyntaxKind.ThisKeyword): ThisExpression;
        createToken(token: SyntaxKind.NullKeyword): NullLiteral;
        createToken(token: SyntaxKind.TrueKeyword): TrueLiteral;
        createToken(token: SyntaxKind.FalseKeyword): FalseLiteral;
        createToken(token: SyntaxKind.EndOfFileToken): EndOfFileToken;
        createToken(token: SyntaxKind.Unknown): Token<SyntaxKind.Unknown>;
        createToken<TKind extends PunctuationSyntaxKind>(token: TKind): PunctuationToken<TKind>;
        createToken<TKind extends KeywordTypeSyntaxKind>(token: TKind): KeywordTypeNode<TKind>;
        createToken<TKind extends ModifierSyntaxKind>(token: TKind): ModifierToken<TKind>;
        createToken<TKind extends KeywordSyntaxKind>(token: TKind): KeywordToken<TKind>;
        createSuper(): SuperExpression;
        createThis(): ThisExpression;
        createNull(): NullLiteral;
        createTrue(): TrueLiteral;
        createFalse(): FalseLiteral;
        createModifier<T extends ModifierSyntaxKind>(kind: T): ModifierToken<T>;
        createModifiersFromModifierFlags(flags: ModifierFlags): Modifier[] | undefined;
        createQualifiedName(left: EntityName, right: string | Identifier): QualifiedName;
        updateQualifiedName(node: QualifiedName, left: EntityName, right: Identifier): QualifiedName;
        createComputedPropertyName(expression: Expression): ComputedPropertyName;
        updateComputedPropertyName(node: ComputedPropertyName, expression: Expression): ComputedPropertyName;
        createTypeParameterDeclaration(modifiers: readonly Modifier[] | undefined, name: string | Identifier, constraint?: TypeNode, defaultType?: TypeNode): TypeParameterDeclaration;
        updateTypeParameterDeclaration(node: TypeParameterDeclaration, modifiers: readonly Modifier[] | undefined, name: Identifier, constraint: TypeNode | undefined, defaultType: TypeNode | undefined): TypeParameterDeclaration;
        createParameterDeclaration(modifiers: readonly ModifierLike[] | undefined, dotDotDotToken: DotDotDotToken | undefined, name: string | BindingName, questionToken?: QuestionToken, type?: TypeNode, initializer?: Expression): ParameterDeclaration;
        updateParameterDeclaration(node: ParameterDeclaration, modifiers: readonly ModifierLike[] | undefined, dotDotDotToken: DotDotDotToken | undefined, name: string | BindingName, questionToken: QuestionToken | undefined, type: TypeNode | undefined, initializer: Expression | undefined): ParameterDeclaration;
        createDecorator(expression: Expression): Decorator;
        updateDecorator(node: Decorator, expression: Expression): Decorator;
        createPropertySignature(modifiers: readonly Modifier[] | undefined, name: PropertyName | string, questionToken: QuestionToken | undefined, type: TypeNode | undefined): PropertySignature;
        updatePropertySignature(node: PropertySignature, modifiers: readonly Modifier[] | undefined, name: PropertyName, questionToken: QuestionToken | undefined, type: TypeNode | undefined): PropertySignature;
        createPropertyDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | PropertyName, questionOrExclamationToken: QuestionToken | ExclamationToken | undefined, type: TypeNode | undefined, initializer: Expression | undefined): PropertyDeclaration;
        updatePropertyDeclaration(node: PropertyDeclaration, modifiers: readonly ModifierLike[] | undefined, name: string | PropertyName, questionOrExclamationToken: QuestionToken | ExclamationToken | undefined, type: TypeNode | undefined, initializer: Expression | undefined): PropertyDeclaration;
        createMethodSignature(modifiers: readonly Modifier[] | undefined, name: string | PropertyName, questionToken: QuestionToken | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined): MethodSignature;
        updateMethodSignature(node: MethodSignature, modifiers: readonly Modifier[] | undefined, name: PropertyName, questionToken: QuestionToken | undefined, typeParameters: NodeArray<TypeParameterDeclaration> | undefined, parameters: NodeArray<ParameterDeclaration>, type: TypeNode | undefined): MethodSignature;
        createMethodDeclaration(modifiers: readonly ModifierLike[] | undefined, asteriskToken: AsteriskToken | undefined, name: string | PropertyName, questionToken: QuestionToken | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): MethodDeclaration;
        updateMethodDeclaration(node: MethodDeclaration, modifiers: readonly ModifierLike[] | undefined, asteriskToken: AsteriskToken | undefined, name: PropertyName, questionToken: QuestionToken | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): MethodDeclaration;
        createConstructorDeclaration(modifiers: readonly ModifierLike[] | undefined, parameters: readonly ParameterDeclaration[], body: Block | undefined): ConstructorDeclaration;
        updateConstructorDeclaration(node: ConstructorDeclaration, modifiers: readonly ModifierLike[] | undefined, parameters: readonly ParameterDeclaration[], body: Block | undefined): ConstructorDeclaration;
        createGetAccessorDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | PropertyName, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): GetAccessorDeclaration;
        updateGetAccessorDeclaration(node: GetAccessorDeclaration, modifiers: readonly ModifierLike[] | undefined, name: PropertyName, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): GetAccessorDeclaration;
        createSetAccessorDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | PropertyName, parameters: readonly ParameterDeclaration[], body: Block | undefined): SetAccessorDeclaration;
        updateSetAccessorDeclaration(node: SetAccessorDeclaration, modifiers: readonly ModifierLike[] | undefined, name: PropertyName, parameters: readonly ParameterDeclaration[], body: Block | undefined): SetAccessorDeclaration;
        createCallSignature(typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined): CallSignatureDeclaration;
        updateCallSignature(node: CallSignatureDeclaration, typeParameters: NodeArray<TypeParameterDeclaration> | undefined, parameters: NodeArray<ParameterDeclaration>, type: TypeNode | undefined): CallSignatureDeclaration;
        createConstructSignature(typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined): ConstructSignatureDeclaration;
        updateConstructSignature(node: ConstructSignatureDeclaration, typeParameters: NodeArray<TypeParameterDeclaration> | undefined, parameters: NodeArray<ParameterDeclaration>, type: TypeNode | undefined): ConstructSignatureDeclaration;
        createIndexSignature(modifiers: readonly ModifierLike[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode): IndexSignatureDeclaration;
        updateIndexSignature(node: IndexSignatureDeclaration, modifiers: readonly ModifierLike[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode): IndexSignatureDeclaration;
        createTemplateLiteralTypeSpan(type: TypeNode, literal: TemplateMiddle | TemplateTail): TemplateLiteralTypeSpan;
        updateTemplateLiteralTypeSpan(node: TemplateLiteralTypeSpan, type: TypeNode, literal: TemplateMiddle | TemplateTail): TemplateLiteralTypeSpan;
        createClassStaticBlockDeclaration(body: Block): ClassStaticBlockDeclaration;
        updateClassStaticBlockDeclaration(node: ClassStaticBlockDeclaration, body: Block): ClassStaticBlockDeclaration;
        createKeywordTypeNode<TKind extends KeywordTypeSyntaxKind>(kind: TKind): KeywordTypeNode<TKind>;
        createTypePredicateNode(assertsModifier: AssertsKeyword | undefined, parameterName: Identifier | ThisTypeNode | string, type: TypeNode | undefined): TypePredicateNode;
        updateTypePredicateNode(node: TypePredicateNode, assertsModifier: AssertsKeyword | undefined, parameterName: Identifier | ThisTypeNode, type: TypeNode | undefined): TypePredicateNode;
        createTypeReferenceNode(typeName: string | EntityName, typeArguments?: readonly TypeNode[]): TypeReferenceNode;
        updateTypeReferenceNode(node: TypeReferenceNode, typeName: EntityName, typeArguments: NodeArray<TypeNode> | undefined): TypeReferenceNode;
        createFunctionTypeNode(typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode): FunctionTypeNode;
        updateFunctionTypeNode(node: FunctionTypeNode, typeParameters: NodeArray<TypeParameterDeclaration> | undefined, parameters: NodeArray<ParameterDeclaration>, type: TypeNode): FunctionTypeNode;
        createConstructorTypeNode(modifiers: readonly Modifier[] | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode): ConstructorTypeNode;
        updateConstructorTypeNode(node: ConstructorTypeNode, modifiers: readonly Modifier[] | undefined, typeParameters: NodeArray<TypeParameterDeclaration> | undefined, parameters: NodeArray<ParameterDeclaration>, type: TypeNode): ConstructorTypeNode;
        createTypeQueryNode(exprName: EntityName, typeArguments?: readonly TypeNode[]): TypeQueryNode;
        updateTypeQueryNode(node: TypeQueryNode, exprName: EntityName, typeArguments?: readonly TypeNode[]): TypeQueryNode;
        createTypeLiteralNode(members: readonly TypeElement[] | undefined): TypeLiteralNode;
        updateTypeLiteralNode(node: TypeLiteralNode, members: NodeArray<TypeElement>): TypeLiteralNode;
        createArrayTypeNode(elementType: TypeNode): ArrayTypeNode;
        updateArrayTypeNode(node: ArrayTypeNode, elementType: TypeNode): ArrayTypeNode;
        createTupleTypeNode(elements: readonly (TypeNode | NamedTupleMember)[]): TupleTypeNode;
        updateTupleTypeNode(node: TupleTypeNode, elements: readonly (TypeNode | NamedTupleMember)[]): TupleTypeNode;
        createNamedTupleMember(dotDotDotToken: DotDotDotToken | undefined, name: Identifier, questionToken: QuestionToken | undefined, type: TypeNode): NamedTupleMember;
        updateNamedTupleMember(node: NamedTupleMember, dotDotDotToken: DotDotDotToken | undefined, name: Identifier, questionToken: QuestionToken | undefined, type: TypeNode): NamedTupleMember;
        createOptionalTypeNode(type: TypeNode): OptionalTypeNode;
        updateOptionalTypeNode(node: OptionalTypeNode, type: TypeNode): OptionalTypeNode;
        createRestTypeNode(type: TypeNode): RestTypeNode;
        updateRestTypeNode(node: RestTypeNode, type: TypeNode): RestTypeNode;
        createUnionTypeNode(types: readonly TypeNode[]): UnionTypeNode;
        updateUnionTypeNode(node: UnionTypeNode, types: NodeArray<TypeNode>): UnionTypeNode;
        createIntersectionTypeNode(types: readonly TypeNode[]): IntersectionTypeNode;
        updateIntersectionTypeNode(node: IntersectionTypeNode, types: NodeArray<TypeNode>): IntersectionTypeNode;
        createConditionalTypeNode(checkType: TypeNode, extendsType: TypeNode, trueType: TypeNode, falseType: TypeNode): ConditionalTypeNode;
        updateConditionalTypeNode(node: ConditionalTypeNode, checkType: TypeNode, extendsType: TypeNode, trueType: TypeNode, falseType: TypeNode): ConditionalTypeNode;
        createInferTypeNode(typeParameter: TypeParameterDeclaration): InferTypeNode;
        updateInferTypeNode(node: InferTypeNode, typeParameter: TypeParameterDeclaration): InferTypeNode;
        createImportTypeNode(argument: TypeNode, attributes?: ImportAttributes, qualifier?: EntityName, typeArguments?: readonly TypeNode[], isTypeOf?: boolean): ImportTypeNode;
        updateImportTypeNode(node: ImportTypeNode, argument: TypeNode, attributes: ImportAttributes | undefined, qualifier: EntityName | undefined, typeArguments: readonly TypeNode[] | undefined, isTypeOf?: boolean): ImportTypeNode;
        createParenthesizedType(type: TypeNode): ParenthesizedTypeNode;
        updateParenthesizedType(node: ParenthesizedTypeNode, type: TypeNode): ParenthesizedTypeNode;
        createThisTypeNode(): ThisTypeNode;
        createTypeOperatorNode(operator: SyntaxKind.KeyOfKeyword | SyntaxKind.UniqueKeyword | SyntaxKind.ReadonlyKeyword, type: TypeNode): TypeOperatorNode;
        updateTypeOperatorNode(node: TypeOperatorNode, type: TypeNode): TypeOperatorNode;
        createIndexedAccessTypeNode(objectType: TypeNode, indexType: TypeNode): IndexedAccessTypeNode;
        updateIndexedAccessTypeNode(node: IndexedAccessTypeNode, objectType: TypeNode, indexType: TypeNode): IndexedAccessTypeNode;
        createMappedTypeNode(readonlyToken: ReadonlyKeyword | PlusToken | MinusToken | undefined, typeParameter: TypeParameterDeclaration, nameType: TypeNode | undefined, questionToken: QuestionToken | PlusToken | MinusToken | undefined, type: TypeNode | undefined, members: NodeArray<TypeElement> | undefined): MappedTypeNode;
        updateMappedTypeNode(node: MappedTypeNode, readonlyToken: ReadonlyKeyword | PlusToken | MinusToken | undefined, typeParameter: TypeParameterDeclaration, nameType: TypeNode | undefined, questionToken: QuestionToken | PlusToken | MinusToken | undefined, type: TypeNode | undefined, members: NodeArray<TypeElement> | undefined): MappedTypeNode;
        createLiteralTypeNode(literal: LiteralTypeNode["literal"]): LiteralTypeNode;
        updateLiteralTypeNode(node: LiteralTypeNode, literal: LiteralTypeNode["literal"]): LiteralTypeNode;
        createTemplateLiteralType(head: TemplateHead, templateSpans: readonly TemplateLiteralTypeSpan[]): TemplateLiteralTypeNode;
        updateTemplateLiteralType(node: TemplateLiteralTypeNode, head: TemplateHead, templateSpans: readonly TemplateLiteralTypeSpan[]): TemplateLiteralTypeNode;
        createObjectBindingPattern(elements: readonly BindingElement[]): ObjectBindingPattern;
        updateObjectBindingPattern(node: ObjectBindingPattern, elements: readonly BindingElement[]): ObjectBindingPattern;
        createArrayBindingPattern(elements: readonly ArrayBindingElement[]): ArrayBindingPattern;
        updateArrayBindingPattern(node: ArrayBindingPattern, elements: readonly ArrayBindingElement[]): ArrayBindingPattern;
        createBindingElement(dotDotDotToken: DotDotDotToken | undefined, propertyName: string | PropertyName | undefined, name: string | BindingName, initializer?: Expression): BindingElement;
        updateBindingElement(node: BindingElement, dotDotDotToken: DotDotDotToken | undefined, propertyName: PropertyName | undefined, name: BindingName, initializer: Expression | undefined): BindingElement;
        createArrayLiteralExpression(elements?: readonly Expression[], multiLine?: boolean): ArrayLiteralExpression;
        updateArrayLiteralExpression(node: ArrayLiteralExpression, elements: readonly Expression[]): ArrayLiteralExpression;
        createObjectLiteralExpression(properties?: readonly ObjectLiteralElementLike[], multiLine?: boolean): ObjectLiteralExpression;
        updateObjectLiteralExpression(node: ObjectLiteralExpression, properties: readonly ObjectLiteralElementLike[]): ObjectLiteralExpression;
        createPropertyAccessExpression(expression: Expression, name: string | MemberName): PropertyAccessExpression;
        updatePropertyAccessExpression(node: PropertyAccessExpression, expression: Expression, name: MemberName): PropertyAccessExpression;
        createPropertyAccessChain(expression: Expression, questionDotToken: QuestionDotToken | undefined, name: string | MemberName): PropertyAccessChain;
        updatePropertyAccessChain(node: PropertyAccessChain, expression: Expression, questionDotToken: QuestionDotToken | undefined, name: MemberName): PropertyAccessChain;
        createElementAccessExpression(expression: Expression, index: number | Expression): ElementAccessExpression;
        updateElementAccessExpression(node: ElementAccessExpression, expression: Expression, argumentExpression: Expression): ElementAccessExpression;
        createElementAccessChain(expression: Expression, questionDotToken: QuestionDotToken | undefined, index: number | Expression): ElementAccessChain;
        updateElementAccessChain(node: ElementAccessChain, expression: Expression, questionDotToken: QuestionDotToken | undefined, argumentExpression: Expression): ElementAccessChain;
        createCallExpression(expression: Expression, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[] | undefined): CallExpression;
        updateCallExpression(node: CallExpression, expression: Expression, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[]): CallExpression;
        createCallChain(expression: Expression, questionDotToken: QuestionDotToken | undefined, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[] | undefined): CallChain;
        updateCallChain(node: CallChain, expression: Expression, questionDotToken: QuestionDotToken | undefined, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[]): CallChain;
        createNewExpression(expression: Expression, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[] | undefined): NewExpression;
        updateNewExpression(node: NewExpression, expression: Expression, typeArguments: readonly TypeNode[] | undefined, argumentsArray: readonly Expression[] | undefined): NewExpression;
        createTaggedTemplateExpression(tag: Expression, typeArguments: readonly TypeNode[] | undefined, template: TemplateLiteral): TaggedTemplateExpression;
        updateTaggedTemplateExpression(node: TaggedTemplateExpression, tag: Expression, typeArguments: readonly TypeNode[] | undefined, template: TemplateLiteral): TaggedTemplateExpression;
        createTypeAssertion(type: TypeNode, expression: Expression): TypeAssertion;
        updateTypeAssertion(node: TypeAssertion, type: TypeNode, expression: Expression): TypeAssertion;
        createParenthesizedExpression(expression: Expression): ParenthesizedExpression;
        updateParenthesizedExpression(node: ParenthesizedExpression, expression: Expression): ParenthesizedExpression;
        createFunctionExpression(modifiers: readonly Modifier[] | undefined, asteriskToken: AsteriskToken | undefined, name: string | Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[] | undefined, type: TypeNode | undefined, body: Block): FunctionExpression;
        updateFunctionExpression(node: FunctionExpression, modifiers: readonly Modifier[] | undefined, asteriskToken: AsteriskToken | undefined, name: Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block): FunctionExpression;
        createArrowFunction(modifiers: readonly Modifier[] | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, equalsGreaterThanToken: EqualsGreaterThanToken | undefined, body: ConciseBody): ArrowFunction;
        updateArrowFunction(node: ArrowFunction, modifiers: readonly Modifier[] | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, equalsGreaterThanToken: EqualsGreaterThanToken, body: ConciseBody): ArrowFunction;
        createDeleteExpression(expression: Expression): DeleteExpression;
        updateDeleteExpression(node: DeleteExpression, expression: Expression): DeleteExpression;
        createTypeOfExpression(expression: Expression): TypeOfExpression;
        updateTypeOfExpression(node: TypeOfExpression, expression: Expression): TypeOfExpression;
        createVoidExpression(expression: Expression): VoidExpression;
        updateVoidExpression(node: VoidExpression, expression: Expression): VoidExpression;
        createAwaitExpression(expression: Expression): AwaitExpression;
        updateAwaitExpression(node: AwaitExpression, expression: Expression): AwaitExpression;
        createPrefixUnaryExpression(operator: PrefixUnaryOperator, operand: Expression): PrefixUnaryExpression;
        updatePrefixUnaryExpression(node: PrefixUnaryExpression, operand: Expression): PrefixUnaryExpression;
        createPostfixUnaryExpression(operand: Expression, operator: PostfixUnaryOperator): PostfixUnaryExpression;
        updatePostfixUnaryExpression(node: PostfixUnaryExpression, operand: Expression): PostfixUnaryExpression;
        createBinaryExpression(left: Expression, operator: BinaryOperator | BinaryOperatorToken, right: Expression): BinaryExpression;
        updateBinaryExpression(node: BinaryExpression, left: Expression, operator: BinaryOperator | BinaryOperatorToken, right: Expression): BinaryExpression;
        createConditionalExpression(condition: Expression, questionToken: QuestionToken | undefined, whenTrue: Expression, colonToken: ColonToken | undefined, whenFalse: Expression): ConditionalExpression;
        updateConditionalExpression(node: ConditionalExpression, condition: Expression, questionToken: QuestionToken, whenTrue: Expression, colonToken: ColonToken, whenFalse: Expression): ConditionalExpression;
        createTemplateExpression(head: TemplateHead, templateSpans: readonly TemplateSpan[]): TemplateExpression;
        updateTemplateExpression(node: TemplateExpression, head: TemplateHead, templateSpans: readonly TemplateSpan[]): TemplateExpression;
        createTemplateHead(text: string, rawText?: string, templateFlags?: TokenFlags): TemplateHead;
        createTemplateHead(text: string | undefined, rawText: string, templateFlags?: TokenFlags): TemplateHead;
        createTemplateMiddle(text: string, rawText?: string, templateFlags?: TokenFlags): TemplateMiddle;
        createTemplateMiddle(text: string | undefined, rawText: string, templateFlags?: TokenFlags): TemplateMiddle;
        createTemplateTail(text: string, rawText?: string, templateFlags?: TokenFlags): TemplateTail;
        createTemplateTail(text: string | undefined, rawText: string, templateFlags?: TokenFlags): TemplateTail;
        createNoSubstitutionTemplateLiteral(text: string, rawText?: string): NoSubstitutionTemplateLiteral;
        createNoSubstitutionTemplateLiteral(text: string | undefined, rawText: string): NoSubstitutionTemplateLiteral;
        createYieldExpression(asteriskToken: AsteriskToken, expression: Expression): YieldExpression;
        createYieldExpression(asteriskToken: undefined, expression: Expression | undefined): YieldExpression;
        updateYieldExpression(node: YieldExpression, asteriskToken: AsteriskToken | undefined, expression: Expression | undefined): YieldExpression;
        createSpreadElement(expression: Expression): SpreadElement;
        updateSpreadElement(node: SpreadElement, expression: Expression): SpreadElement;
        createClassExpression(modifiers: readonly ModifierLike[] | undefined, name: string | Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly ClassElement[]): ClassExpression;
        updateClassExpression(node: ClassExpression, modifiers: readonly ModifierLike[] | undefined, name: Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly ClassElement[]): ClassExpression;
        createOmittedExpression(): OmittedExpression;
        createExpressionWithTypeArguments(expression: Expression, typeArguments: readonly TypeNode[] | undefined): ExpressionWithTypeArguments;
        updateExpressionWithTypeArguments(node: ExpressionWithTypeArguments, expression: Expression, typeArguments: readonly TypeNode[] | undefined): ExpressionWithTypeArguments;
        createAsExpression(expression: Expression, type: TypeNode): AsExpression;
        updateAsExpression(node: AsExpression, expression: Expression, type: TypeNode): AsExpression;
        createNonNullExpression(expression: Expression): NonNullExpression;
        updateNonNullExpression(node: NonNullExpression, expression: Expression): NonNullExpression;
        createNonNullChain(expression: Expression): NonNullChain;
        updateNonNullChain(node: NonNullChain, expression: Expression): NonNullChain;
        createMetaProperty(keywordToken: MetaProperty["keywordToken"], name: Identifier): MetaProperty;
        updateMetaProperty(node: MetaProperty, name: Identifier): MetaProperty;
        createSatisfiesExpression(expression: Expression, type: TypeNode): SatisfiesExpression;
        updateSatisfiesExpression(node: SatisfiesExpression, expression: Expression, type: TypeNode): SatisfiesExpression;
        createTemplateSpan(expression: Expression, literal: TemplateMiddle | TemplateTail): TemplateSpan;
        updateTemplateSpan(node: TemplateSpan, expression: Expression, literal: TemplateMiddle | TemplateTail): TemplateSpan;
        createSemicolonClassElement(): SemicolonClassElement;
        createBlock(statements: readonly Statement[], multiLine?: boolean): Block;
        updateBlock(node: Block, statements: readonly Statement[]): Block;
        createVariableStatement(modifiers: readonly ModifierLike[] | undefined, declarationList: VariableDeclarationList | readonly VariableDeclaration[]): VariableStatement;
        updateVariableStatement(node: VariableStatement, modifiers: readonly ModifierLike[] | undefined, declarationList: VariableDeclarationList): VariableStatement;
        createEmptyStatement(): EmptyStatement;
        createExpressionStatement(expression: Expression): ExpressionStatement;
        updateExpressionStatement(node: ExpressionStatement, expression: Expression): ExpressionStatement;
        createIfStatement(expression: Expression, thenStatement: Statement, elseStatement?: Statement): IfStatement;
        updateIfStatement(node: IfStatement, expression: Expression, thenStatement: Statement, elseStatement: Statement | undefined): IfStatement;
        createDoStatement(statement: Statement, expression: Expression): DoStatement;
        updateDoStatement(node: DoStatement, statement: Statement, expression: Expression): DoStatement;
        createWhileStatement(expression: Expression, statement: Statement): WhileStatement;
        updateWhileStatement(node: WhileStatement, expression: Expression, statement: Statement): WhileStatement;
        createForStatement(initializer: ForInitializer | undefined, condition: Expression | undefined, incrementor: Expression | undefined, statement: Statement): ForStatement;
        updateForStatement(node: ForStatement, initializer: ForInitializer | undefined, condition: Expression | undefined, incrementor: Expression | undefined, statement: Statement): ForStatement;
        createForInStatement(initializer: ForInitializer, expression: Expression, statement: Statement): ForInStatement;
        updateForInStatement(node: ForInStatement, initializer: ForInitializer, expression: Expression, statement: Statement): ForInStatement;
        createForOfStatement(awaitModifier: AwaitKeyword | undefined, initializer: ForInitializer, expression: Expression, statement: Statement): ForOfStatement;
        updateForOfStatement(node: ForOfStatement, awaitModifier: AwaitKeyword | undefined, initializer: ForInitializer, expression: Expression, statement: Statement): ForOfStatement;
        createContinueStatement(label?: string | Identifier): ContinueStatement;
        updateContinueStatement(node: ContinueStatement, label: Identifier | undefined): ContinueStatement;
        createBreakStatement(label?: string | Identifier): BreakStatement;
        updateBreakStatement(node: BreakStatement, label: Identifier | undefined): BreakStatement;
        createReturnStatement(expression?: Expression): ReturnStatement;
        updateReturnStatement(node: ReturnStatement, expression: Expression | undefined): ReturnStatement;
        createWithStatement(expression: Expression, statement: Statement): WithStatement;
        updateWithStatement(node: WithStatement, expression: Expression, statement: Statement): WithStatement;
        createSwitchStatement(expression: Expression, caseBlock: CaseBlock): SwitchStatement;
        updateSwitchStatement(node: SwitchStatement, expression: Expression, caseBlock: CaseBlock): SwitchStatement;
        createLabeledStatement(label: string | Identifier, statement: Statement): LabeledStatement;
        updateLabeledStatement(node: LabeledStatement, label: Identifier, statement: Statement): LabeledStatement;
        createThrowStatement(expression: Expression): ThrowStatement;
        updateThrowStatement(node: ThrowStatement, expression: Expression): ThrowStatement;
        createTryStatement(tryBlock: Block, catchClause: CatchClause | undefined, finallyBlock: Block | undefined): TryStatement;
        updateTryStatement(node: TryStatement, tryBlock: Block, catchClause: CatchClause | undefined, finallyBlock: Block | undefined): TryStatement;
        createDebuggerStatement(): DebuggerStatement;
        createVariableDeclaration(name: string | BindingName, exclamationToken?: ExclamationToken, type?: TypeNode, initializer?: Expression): VariableDeclaration;
        updateVariableDeclaration(node: VariableDeclaration, name: BindingName, exclamationToken: ExclamationToken | undefined, type: TypeNode | undefined, initializer: Expression | undefined): VariableDeclaration;
        createVariableDeclarationList(declarations: readonly VariableDeclaration[], flags?: NodeFlags): VariableDeclarationList;
        updateVariableDeclarationList(node: VariableDeclarationList, declarations: readonly VariableDeclaration[]): VariableDeclarationList;
        createFunctionDeclaration(modifiers: readonly ModifierLike[] | undefined, asteriskToken: AsteriskToken | undefined, name: string | Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): FunctionDeclaration;
        updateFunctionDeclaration(node: FunctionDeclaration, modifiers: readonly ModifierLike[] | undefined, asteriskToken: AsteriskToken | undefined, name: Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined, body: Block | undefined): FunctionDeclaration;
        createClassDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly ClassElement[]): ClassDeclaration;
        updateClassDeclaration(node: ClassDeclaration, modifiers: readonly ModifierLike[] | undefined, name: Identifier | undefined, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly ClassElement[]): ClassDeclaration;
        createInterfaceDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | Identifier, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly TypeElement[]): InterfaceDeclaration;
        updateInterfaceDeclaration(node: InterfaceDeclaration, modifiers: readonly ModifierLike[] | undefined, name: Identifier, typeParameters: readonly TypeParameterDeclaration[] | undefined, heritageClauses: readonly HeritageClause[] | undefined, members: readonly TypeElement[]): InterfaceDeclaration;
        createTypeAliasDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | Identifier, typeParameters: readonly TypeParameterDeclaration[] | undefined, type: TypeNode): TypeAliasDeclaration;
        updateTypeAliasDeclaration(node: TypeAliasDeclaration, modifiers: readonly ModifierLike[] | undefined, name: Identifier, typeParameters: readonly TypeParameterDeclaration[] | undefined, type: TypeNode): TypeAliasDeclaration;
        createEnumDeclaration(modifiers: readonly ModifierLike[] | undefined, name: string | Identifier, members: readonly EnumMember[]): EnumDeclaration;
        updateEnumDeclaration(node: EnumDeclaration, modifiers: readonly ModifierLike[] | undefined, name: Identifier, members: readonly EnumMember[]): EnumDeclaration;
        createModuleDeclaration(modifiers: readonly ModifierLike[] | undefined, name: ModuleName, body: ModuleBody | undefined, flags?: NodeFlags): ModuleDeclaration;
        updateModuleDeclaration(node: ModuleDeclaration, modifiers: readonly ModifierLike[] | undefined, name: ModuleName, body: ModuleBody | undefined): ModuleDeclaration;
        createModuleBlock(statements: readonly Statement[]): ModuleBlock;
        updateModuleBlock(node: ModuleBlock, statements: readonly Statement[]): ModuleBlock;
        createCaseBlock(clauses: readonly CaseOrDefaultClause[]): CaseBlock;
        updateCaseBlock(node: CaseBlock, clauses: readonly CaseOrDefaultClause[]): CaseBlock;
        createNamespaceExportDeclaration(name: string | Identifier): NamespaceExportDeclaration;
        updateNamespaceExportDeclaration(node: NamespaceExportDeclaration, name: Identifier): NamespaceExportDeclaration;
        createImportEqualsDeclaration(modifiers: readonly ModifierLike[] | undefined, isTypeOnly: boolean, name: string | Identifier, moduleReference: ModuleReference): ImportEqualsDeclaration;
        updateImportEqualsDeclaration(node: ImportEqualsDeclaration, modifiers: readonly ModifierLike[] | undefined, isTypeOnly: boolean, name: Identifier, moduleReference: ModuleReference): ImportEqualsDeclaration;
        createImportDeclaration(modifiers: readonly ModifierLike[] | undefined, importClause: ImportClause | undefined, moduleSpecifier: Expression, attributes?: ImportAttributes): ImportDeclaration;
        updateImportDeclaration(node: ImportDeclaration, modifiers: readonly ModifierLike[] | undefined, importClause: ImportClause | undefined, moduleSpecifier: Expression, attributes: ImportAttributes | undefined): ImportDeclaration;
        createImportClause(phaseModifier: ImportPhaseModifierSyntaxKind | undefined, name: Identifier | undefined, namedBindings: NamedImportBindings | undefined): ImportClause;
createImportClause(isTypeOnly: boolean, name: Identifier | undefined, namedBindings: NamedImportBindings | undefined): ImportClause;
        updateImportClause(node: ImportClause, phaseModifier: ImportPhaseModifierSyntaxKind | undefined, name: Identifier | undefined, namedBindings: NamedImportBindings | undefined): ImportClause;
updateImportClause(node: ImportClause, isTypeOnly: boolean, name: Identifier | undefined, namedBindings: NamedImportBindings | undefined): ImportClause;
createAssertClause(elements: NodeArray<AssertEntry>, multiLine?: boolean): AssertClause;
updateAssertClause(node: AssertClause, elements: NodeArray<AssertEntry>, multiLine?: boolean): AssertClause;
createAssertEntry(name: AssertionKey, value: Expression): AssertEntry;
updateAssertEntry(node: AssertEntry, name: AssertionKey, value: Expression): AssertEntry;
createImportTypeAssertionContainer(clause: AssertClause, multiLine?: boolean): ImportTypeAssertionContainer;
updateImportTypeAssertionContainer(node: ImportTypeAssertionContainer, clause: AssertClause, multiLine?: boolean): ImportTypeAssertionContainer;
        createImportAttributes(elements: NodeArray<ImportAttribute>, multiLine?: boolean): ImportAttributes;
        updateImportAttributes(node: ImportAttributes, elements: NodeArray<ImportAttribute>, multiLine?: boolean): ImportAttributes;
        createImportAttribute(name: ImportAttributeName, value: Expression): ImportAttribute;
        updateImportAttribute(node: ImportAttribute, name: ImportAttributeName, value: Expression): ImportAttribute;
        createNamespaceImport(name: Identifier): NamespaceImport;
        updateNamespaceImport(node: NamespaceImport, name: Identifier): NamespaceImport;
        createNamespaceExport(name: ModuleExportName): NamespaceExport;
        updateNamespaceExport(node: NamespaceExport, name: ModuleExportName): NamespaceExport;
        createNamedImports(elements: readonly ImportSpecifier[]): NamedImports;
        updateNamedImports(node: NamedImports, elements: readonly ImportSpecifier[]): NamedImports;
        createImportSpecifier(isTypeOnly: boolean, propertyName: ModuleExportName | undefined, name: Identifier): ImportSpecifier;
        updateImportSpecifier(node: ImportSpecifier, isTypeOnly: boolean, propertyName: ModuleExportName | undefined, name: Identifier): ImportSpecifier;
        createExportAssignment(modifiers: readonly ModifierLike[] | undefined, isExportEquals: boolean | undefined, expression: Expression): ExportAssignment;
        updateExportAssignment(node: ExportAssignment, modifiers: readonly ModifierLike[] | undefined, expression: Expression): ExportAssignment;
        createExportDeclaration(modifiers: readonly ModifierLike[] | undefined, isTypeOnly: boolean, exportClause: NamedExportBindings | undefined, moduleSpecifier?: Expression, attributes?: ImportAttributes): ExportDeclaration;
        updateExportDeclaration(node: ExportDeclaration, modifiers: readonly ModifierLike[] | undefined, isTypeOnly: boolean, exportClause: NamedExportBindings | undefined, moduleSpecifier: Expression | undefined, attributes: ImportAttributes | undefined): ExportDeclaration;
        createNamedExports(elements: readonly ExportSpecifier[]): NamedExports;
        updateNamedExports(node: NamedExports, elements: readonly ExportSpecifier[]): NamedExports;
        createExportSpecifier(isTypeOnly: boolean, propertyName: string | ModuleExportName | undefined, name: string | ModuleExportName): ExportSpecifier;
        updateExportSpecifier(node: ExportSpecifier, isTypeOnly: boolean, propertyName: ModuleExportName | undefined, name: ModuleExportName): ExportSpecifier;
        createExternalModuleReference(expression: Expression): ExternalModuleReference;
        updateExternalModuleReference(node: ExternalModuleReference, expression: Expression): ExternalModuleReference;
        createJSDocAllType(): JSDocAllType;
        createJSDocUnknownType(): JSDocUnknownType;
        createJSDocNonNullableType(type: TypeNode, postfix?: boolean): JSDocNonNullableType;
        updateJSDocNonNullableType(node: JSDocNonNullableType, type: TypeNode): JSDocNonNullableType;
        createJSDocNullableType(type: TypeNode, postfix?: boolean): JSDocNullableType;
        updateJSDocNullableType(node: JSDocNullableType, type: TypeNode): JSDocNullableType;
        createJSDocOptionalType(type: TypeNode): JSDocOptionalType;
        updateJSDocOptionalType(node: JSDocOptionalType, type: TypeNode): JSDocOptionalType;
        createJSDocFunctionType(parameters: readonly ParameterDeclaration[], type: TypeNode | undefined): JSDocFunctionType;
        updateJSDocFunctionType(node: JSDocFunctionType, parameters: readonly ParameterDeclaration[], type: TypeNode | undefined): JSDocFunctionType;
        createJSDocVariadicType(type: TypeNode): JSDocVariadicType;
        updateJSDocVariadicType(node: JSDocVariadicType, type: TypeNode): JSDocVariadicType;
        createJSDocNamepathType(type: TypeNode): JSDocNamepathType;
        updateJSDocNamepathType(node: JSDocNamepathType, type: TypeNode): JSDocNamepathType;
        createJSDocTypeExpression(type: TypeNode): JSDocTypeExpression;
        updateJSDocTypeExpression(node: JSDocTypeExpression, type: TypeNode): JSDocTypeExpression;
        createJSDocNameReference(name: EntityName | JSDocMemberName): JSDocNameReference;
        updateJSDocNameReference(node: JSDocNameReference, name: EntityName | JSDocMemberName): JSDocNameReference;
        createJSDocMemberName(left: EntityName | JSDocMemberName, right: Identifier): JSDocMemberName;
        updateJSDocMemberName(node: JSDocMemberName, left: EntityName | JSDocMemberName, right: Identifier): JSDocMemberName;
        createJSDocLink(name: EntityName | JSDocMemberName | undefined, text: string): JSDocLink;
        updateJSDocLink(node: JSDocLink, name: EntityName | JSDocMemberName | undefined, text: string): JSDocLink;
        createJSDocLinkCode(name: EntityName | JSDocMemberName | undefined, text: string): JSDocLinkCode;
        updateJSDocLinkCode(node: JSDocLinkCode, name: EntityName | JSDocMemberName | undefined, text: string): JSDocLinkCode;
        createJSDocLinkPlain(name: EntityName | JSDocMemberName | undefined, text: string): JSDocLinkPlain;
        updateJSDocLinkPlain(node: JSDocLinkPlain, name: EntityName | JSDocMemberName | undefined, text: string): JSDocLinkPlain;
        createJSDocTypeLiteral(jsDocPropertyTags?: readonly JSDocPropertyLikeTag[], isArrayType?: boolean): JSDocTypeLiteral;
        updateJSDocTypeLiteral(node: JSDocTypeLiteral, jsDocPropertyTags: readonly JSDocPropertyLikeTag[] | undefined, isArrayType: boolean | undefined): JSDocTypeLiteral;
        createJSDocSignature(typeParameters: readonly JSDocTemplateTag[] | undefined, parameters: readonly JSDocParameterTag[], type?: JSDocReturnTag): JSDocSignature;
        updateJSDocSignature(node: JSDocSignature, typeParameters: readonly JSDocTemplateTag[] | undefined, parameters: readonly JSDocParameterTag[], type: JSDocReturnTag | undefined): JSDocSignature;
        createJSDocTemplateTag(tagName: Identifier | undefined, constraint: JSDocTypeExpression | undefined, typeParameters: readonly TypeParameterDeclaration[], comment?: string | NodeArray<JSDocComment>): JSDocTemplateTag;
        updateJSDocTemplateTag(node: JSDocTemplateTag, tagName: Identifier | undefined, constraint: JSDocTypeExpression | undefined, typeParameters: readonly TypeParameterDeclaration[], comment: string | NodeArray<JSDocComment> | undefined): JSDocTemplateTag;
        createJSDocTypedefTag(tagName: Identifier | undefined, typeExpression?: JSDocTypeExpression | JSDocTypeLiteral, fullName?: Identifier | JSDocNamespaceDeclaration, comment?: string | NodeArray<JSDocComment>): JSDocTypedefTag;
        updateJSDocTypedefTag(node: JSDocTypedefTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression | JSDocTypeLiteral | undefined, fullName: Identifier | JSDocNamespaceDeclaration | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocTypedefTag;
        createJSDocParameterTag(tagName: Identifier | undefined, name: EntityName, isBracketed: boolean, typeExpression?: JSDocTypeExpression, isNameFirst?: boolean, comment?: string | NodeArray<JSDocComment>): JSDocParameterTag;
        updateJSDocParameterTag(node: JSDocParameterTag, tagName: Identifier | undefined, name: EntityName, isBracketed: boolean, typeExpression: JSDocTypeExpression | undefined, isNameFirst: boolean, comment: string | NodeArray<JSDocComment> | undefined): JSDocParameterTag;
        createJSDocPropertyTag(tagName: Identifier | undefined, name: EntityName, isBracketed: boolean, typeExpression?: JSDocTypeExpression, isNameFirst?: boolean, comment?: string | NodeArray<JSDocComment>): JSDocPropertyTag;
        updateJSDocPropertyTag(node: JSDocPropertyTag, tagName: Identifier | undefined, name: EntityName, isBracketed: boolean, typeExpression: JSDocTypeExpression | undefined, isNameFirst: boolean, comment: string | NodeArray<JSDocComment> | undefined): JSDocPropertyTag;
        createJSDocTypeTag(tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment?: string | NodeArray<JSDocComment>): JSDocTypeTag;
        updateJSDocTypeTag(node: JSDocTypeTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment: string | NodeArray<JSDocComment> | undefined): JSDocTypeTag;
        createJSDocSeeTag(tagName: Identifier | undefined, nameExpression: JSDocNameReference | undefined, comment?: string | NodeArray<JSDocComment>): JSDocSeeTag;
        updateJSDocSeeTag(node: JSDocSeeTag, tagName: Identifier | undefined, nameExpression: JSDocNameReference | undefined, comment?: string | NodeArray<JSDocComment>): JSDocSeeTag;
        createJSDocReturnTag(tagName: Identifier | undefined, typeExpression?: JSDocTypeExpression, comment?: string | NodeArray<JSDocComment>): JSDocReturnTag;
        updateJSDocReturnTag(node: JSDocReturnTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocReturnTag;
        createJSDocThisTag(tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment?: string | NodeArray<JSDocComment>): JSDocThisTag;
        updateJSDocThisTag(node: JSDocThisTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocThisTag;
        createJSDocEnumTag(tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment?: string | NodeArray<JSDocComment>): JSDocEnumTag;
        updateJSDocEnumTag(node: JSDocEnumTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment: string | NodeArray<JSDocComment> | undefined): JSDocEnumTag;
        createJSDocCallbackTag(tagName: Identifier | undefined, typeExpression: JSDocSignature, fullName?: Identifier | JSDocNamespaceDeclaration, comment?: string | NodeArray<JSDocComment>): JSDocCallbackTag;
        updateJSDocCallbackTag(node: JSDocCallbackTag, tagName: Identifier | undefined, typeExpression: JSDocSignature, fullName: Identifier | JSDocNamespaceDeclaration | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocCallbackTag;
        createJSDocOverloadTag(tagName: Identifier | undefined, typeExpression: JSDocSignature, comment?: string | NodeArray<JSDocComment>): JSDocOverloadTag;
        updateJSDocOverloadTag(node: JSDocOverloadTag, tagName: Identifier | undefined, typeExpression: JSDocSignature, comment: string | NodeArray<JSDocComment> | undefined): JSDocOverloadTag;
        createJSDocAugmentsTag(tagName: Identifier | undefined, className: JSDocAugmentsTag["class"], comment?: string | NodeArray<JSDocComment>): JSDocAugmentsTag;
        updateJSDocAugmentsTag(node: JSDocAugmentsTag, tagName: Identifier | undefined, className: JSDocAugmentsTag["class"], comment: string | NodeArray<JSDocComment> | undefined): JSDocAugmentsTag;
        createJSDocImplementsTag(tagName: Identifier | undefined, className: JSDocImplementsTag["class"], comment?: string | NodeArray<JSDocComment>): JSDocImplementsTag;
        updateJSDocImplementsTag(node: JSDocImplementsTag, tagName: Identifier | undefined, className: JSDocImplementsTag["class"], comment: string | NodeArray<JSDocComment> | undefined): JSDocImplementsTag;
        createJSDocAuthorTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocAuthorTag;
        updateJSDocAuthorTag(node: JSDocAuthorTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocAuthorTag;
        createJSDocClassTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocClassTag;
        updateJSDocClassTag(node: JSDocClassTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocClassTag;
        createJSDocPublicTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocPublicTag;
        updateJSDocPublicTag(node: JSDocPublicTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocPublicTag;
        createJSDocPrivateTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocPrivateTag;
        updateJSDocPrivateTag(node: JSDocPrivateTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocPrivateTag;
        createJSDocProtectedTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocProtectedTag;
        updateJSDocProtectedTag(node: JSDocProtectedTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocProtectedTag;
        createJSDocReadonlyTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocReadonlyTag;
        updateJSDocReadonlyTag(node: JSDocReadonlyTag, tagName: Identifier | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocReadonlyTag;
        createJSDocUnknownTag(tagName: Identifier, comment?: string | NodeArray<JSDocComment>): JSDocUnknownTag;
        updateJSDocUnknownTag(node: JSDocUnknownTag, tagName: Identifier, comment: string | NodeArray<JSDocComment> | undefined): JSDocUnknownTag;
        createJSDocDeprecatedTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocDeprecatedTag;
        updateJSDocDeprecatedTag(node: JSDocDeprecatedTag, tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocDeprecatedTag;
        createJSDocOverrideTag(tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocOverrideTag;
        updateJSDocOverrideTag(node: JSDocOverrideTag, tagName: Identifier | undefined, comment?: string | NodeArray<JSDocComment>): JSDocOverrideTag;
        createJSDocThrowsTag(tagName: Identifier, typeExpression: JSDocTypeExpression | undefined, comment?: string | NodeArray<JSDocComment>): JSDocThrowsTag;
        updateJSDocThrowsTag(node: JSDocThrowsTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression | undefined, comment?: string | NodeArray<JSDocComment> | undefined): JSDocThrowsTag;
        createJSDocSatisfiesTag(tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment?: string | NodeArray<JSDocComment>): JSDocSatisfiesTag;
        updateJSDocSatisfiesTag(node: JSDocSatisfiesTag, tagName: Identifier | undefined, typeExpression: JSDocTypeExpression, comment: string | NodeArray<JSDocComment> | undefined): JSDocSatisfiesTag;
        createJSDocImportTag(tagName: Identifier | undefined, importClause: ImportClause | undefined, moduleSpecifier: Expression, attributes?: ImportAttributes, comment?: string | NodeArray<JSDocComment>): JSDocImportTag;
        updateJSDocImportTag(node: JSDocImportTag, tagName: Identifier | undefined, importClause: ImportClause | undefined, moduleSpecifier: Expression, attributes: ImportAttributes | undefined, comment: string | NodeArray<JSDocComment> | undefined): JSDocImportTag;
        createJSDocText(text: string): JSDocText;
        updateJSDocText(node: JSDocText, text: string): JSDocText;
        createJSDocComment(comment?: string | NodeArray<JSDocComment> | undefined, tags?: readonly JSDocTag[] | undefined): JSDoc;
        updateJSDocComment(node: JSDoc, comment: string | NodeArray<JSDocComment> | undefined, tags: readonly JSDocTag[] | undefined): JSDoc;
        createJsxElement(openingElement: JsxOpeningElement, children: readonly JsxChild[], closingElement: JsxClosingElement): JsxElement;
        updateJsxElement(node: JsxElement, openingElement: JsxOpeningElement, children: readonly JsxChild[], closingElement: JsxClosingElement): JsxElement;
        createJsxSelfClosingElement(tagName: JsxTagNameExpression, typeArguments: readonly TypeNode[] | undefined, attributes: JsxAttributes): JsxSelfClosingElement;
        updateJsxSelfClosingElement(node: JsxSelfClosingElement, tagName: JsxTagNameExpression, typeArguments: readonly TypeNode[] | undefined, attributes: JsxAttributes): JsxSelfClosingElement;
        createJsxOpeningElement(tagName: JsxTagNameExpression, typeArguments: readonly TypeNode[] | undefined, attributes: JsxAttributes): JsxOpeningElement;
        updateJsxOpeningElement(node: JsxOpeningElement, tagName: JsxTagNameExpression, typeArguments: readonly TypeNode[] | undefined, attributes: JsxAttributes): JsxOpeningElement;
        createJsxClosingElement(tagName: JsxTagNameExpression): JsxClosingElement;
        updateJsxClosingElement(node: JsxClosingElement, tagName: JsxTagNameExpression): JsxClosingElement;
        createJsxFragment(openingFragment: JsxOpeningFragment, children: readonly JsxChild[], closingFragment: JsxClosingFragment): JsxFragment;
        createJsxText(text: string, containsOnlyTriviaWhiteSpaces?: boolean): JsxText;
        updateJsxText(node: JsxText, text: string, containsOnlyTriviaWhiteSpaces?: boolean): JsxText;
        createJsxOpeningFragment(): JsxOpeningFragment;
        createJsxJsxClosingFragment(): JsxClosingFragment;
        updateJsxFragment(node: JsxFragment, openingFragment: JsxOpeningFragment, children: readonly JsxChild[], closingFragment: JsxClosingFragment): JsxFragment;
        createJsxAttribute(name: JsxAttributeName, initializer: JsxAttributeValue | undefined): JsxAttribute;
        updateJsxAttribute(node: JsxAttribute, name: JsxAttributeName, initializer: JsxAttributeValue | undefined): JsxAttribute;
        createJsxAttributes(properties: readonly JsxAttributeLike[]): JsxAttributes;
        updateJsxAttributes(node: JsxAttributes, properties: readonly JsxAttributeLike[]): JsxAttributes;
        createJsxSpreadAttribute(expression: Expression): JsxSpreadAttribute;
        updateJsxSpreadAttribute(node: JsxSpreadAttribute, expression: Expression): JsxSpreadAttribute;
        createJsxExpression(dotDotDotToken: DotDotDotToken | undefined, expression: Expression | undefined): JsxExpression;
        updateJsxExpression(node: JsxExpression, expression: Expression | undefined): JsxExpression;
        createJsxNamespacedName(namespace: Identifier, name: Identifier): JsxNamespacedName;
        updateJsxNamespacedName(node: JsxNamespacedName, namespace: Identifier, name: Identifier): JsxNamespacedName;
        createCaseClause(expression: Expression, statements: readonly Statement[]): CaseClause;
        updateCaseClause(node: CaseClause, expression: Expression, statements: readonly Statement[]): CaseClause;
        createDefaultClause(statements: readonly Statement[]): DefaultClause;
        updateDefaultClause(node: DefaultClause, statements: readonly Statement[]): DefaultClause;
        createHeritageClause(token: HeritageClause["token"], types: readonly ExpressionWithTypeArguments[]): HeritageClause;
        updateHeritageClause(node: HeritageClause, types: readonly ExpressionWithTypeArguments[]): HeritageClause;
        createCatchClause(variableDeclaration: string | BindingName | VariableDeclaration | undefined, block: Block): CatchClause;
        updateCatchClause(node: CatchClause, variableDeclaration: VariableDeclaration | undefined, block: Block): CatchClause;
        createPropertyAssignment(name: string | PropertyName, initializer: Expression): PropertyAssignment;
        updatePropertyAssignment(node: PropertyAssignment, name: PropertyName, initializer: Expression): PropertyAssignment;
        createShorthandPropertyAssignment(name: string | Identifier, objectAssignmentInitializer?: Expression): ShorthandPropertyAssignment;
        updateShorthandPropertyAssignment(node: ShorthandPropertyAssignment, name: Identifier, objectAssignmentInitializer: Expression | undefined): ShorthandPropertyAssignment;
        createSpreadAssignment(expression: Expression): SpreadAssignment;
        updateSpreadAssignment(node: SpreadAssignment, expression: Expression): SpreadAssignment;
        createEnumMember(name: string | PropertyName, initializer?: Expression): EnumMember;
        updateEnumMember(node: EnumMember, name: PropertyName, initializer: Expression | undefined): EnumMember;
        createSourceFile(statements: readonly Statement[], endOfFileToken: EndOfFileToken, flags: NodeFlags): SourceFile;
        updateSourceFile(node: SourceFile, statements: readonly Statement[], isDeclarationFile?: boolean, referencedFiles?: readonly FileReference[], typeReferences?: readonly FileReference[], hasNoDefaultLib?: boolean, libReferences?: readonly FileReference[]): SourceFile;
        createNotEmittedStatement(original: Node): NotEmittedStatement;
        createNotEmittedTypeElement(): NotEmittedTypeElement;
        createPartiallyEmittedExpression(expression: Expression, original?: Node): PartiallyEmittedExpression;
        updatePartiallyEmittedExpression(node: PartiallyEmittedExpression, expression: Expression): PartiallyEmittedExpression;
        createCommaListExpression(elements: readonly Expression[]): CommaListExpression;
        updateCommaListExpression(node: CommaListExpression, elements: readonly Expression[]): CommaListExpression;
        createBundle(sourceFiles: readonly SourceFile[]): Bundle;
        updateBundle(node: Bundle, sourceFiles: readonly SourceFile[]): Bundle;
        createComma(left: Expression, right: Expression): BinaryExpression;
        createAssignment(left: ObjectLiteralExpression | ArrayLiteralExpression, right: Expression): DestructuringAssignment;
        createAssignment(left: Expression, right: Expression): AssignmentExpression<EqualsToken>;
        createLogicalOr(left: Expression, right: Expression): BinaryExpression;
        createLogicalAnd(left: Expression, right: Expression): BinaryExpression;
        createBitwiseOr(left: Expression, right: Expression): BinaryExpression;
        createBitwiseXor(left: Expression, right: Expression): BinaryExpression;
        createBitwiseAnd(left: Expression, right: Expression): BinaryExpression;
        createStrictEquality(left: Expression, right: Expression): BinaryExpression;
        createStrictInequality(left: Expression, right: Expression): BinaryExpression;
        createEquality(left: Expression, right: Expression): BinaryExpression;
        createInequality(left: Expression, right: Expression): BinaryExpression;
        createLessThan(left: Expression, right: Expression): BinaryExpression;
        createLessThanEquals(left: Expression, right: Expression): BinaryExpression;
        createGreaterThan(left: Expression, right: Expression): BinaryExpression;
        createGreaterThanEquals(left: Expression, right: Expression): BinaryExpression;
        createLeftShift(left: Expression, right: Expression): BinaryExpression;
        createRightShift(left: Expression, right: Expression): BinaryExpression;
        createUnsignedRightShift(left: Expression, right: Expression): BinaryExpression;
        createAdd(left: Expression, right: Expression): BinaryExpression;
        createSubtract(left: Expression, right: Expression): BinaryExpression;
        createMultiply(left: Expression, right: Expression): BinaryExpression;
        createDivide(left: Expression, right: Expression): BinaryExpression;
        createModulo(left: Expression, right: Expression): BinaryExpression;
        createExponent(left: Expression, right: Expression): BinaryExpression;
        createPrefixPlus(operand: Expression): PrefixUnaryExpression;
        createPrefixMinus(operand: Expression): PrefixUnaryExpression;
        createPrefixIncrement(operand: Expression): PrefixUnaryExpression;
        createPrefixDecrement(operand: Expression): PrefixUnaryExpression;
        createBitwiseNot(operand: Expression): PrefixUnaryExpression;
        createLogicalNot(operand: Expression): PrefixUnaryExpression;
        createPostfixIncrement(operand: Expression): PostfixUnaryExpression;
        createPostfixDecrement(operand: Expression): PostfixUnaryExpression;
        createImmediatelyInvokedFunctionExpression(statements: readonly Statement[]): CallExpression;
        createImmediatelyInvokedFunctionExpression(statements: readonly Statement[], param: ParameterDeclaration, paramValue: Expression): CallExpression;
        createImmediatelyInvokedArrowFunction(statements: readonly Statement[]): ImmediatelyInvokedArrowFunction;
        createImmediatelyInvokedArrowFunction(statements: readonly Statement[], param: ParameterDeclaration, paramValue: Expression): ImmediatelyInvokedArrowFunction;
        createVoidZero(): VoidExpression;
        createExportDefault(expression: Expression): ExportAssignment;
        createExternalModuleExport(exportName: Identifier): ExportDeclaration;
        restoreOuterExpressions(outerExpression: Expression | undefined, innerExpression: Expression, kinds?: OuterExpressionKinds): Expression;
        replaceModifiers<T extends HasModifiers>(node: T, modifiers: readonly Modifier[] | ModifierFlags | undefined): T;
        replaceDecoratorsAndModifiers<T extends HasModifiers & HasDecorators>(node: T, modifiers: readonly ModifierLike[] | undefined): T;
        replacePropertyName<T extends AccessorDeclaration | MethodDeclaration | MethodSignature | PropertyDeclaration | PropertySignature | PropertyAssignment>(node: T, name: T["name"]): T;
    }
    interface CoreTransformationContext {
        readonly factory: NodeFactory;
        getCompilerOptions(): CompilerOptions;
        startLexicalEnvironment(): void;
        suspendLexicalEnvironment(): void;
        resumeLexicalEnvironment(): void;
        endLexicalEnvironment(): Statement[] | undefined;
        hoistFunctionDeclaration(node: FunctionDeclaration): void;
        hoistVariableDeclaration(node: Identifier): void;
    }
    interface TransformationContext extends CoreTransformationContext {
        requestEmitHelper(helper: EmitHelper): void;
        readEmitHelpers(): EmitHelper[] | undefined;
        enableSubstitution(kind: SyntaxKind): void;
        isSubstitutionEnabled(node: Node): boolean;
        onSubstituteNode: (hint: EmitHint, node: Node) => Node;
        enableEmitNotification(kind: SyntaxKind): void;
        isEmitNotificationEnabled(node: Node): boolean;
        onEmitNode: (hint: EmitHint, node: Node, emitCallback: (hint: EmitHint, node: Node) => void) => void;
    }
    interface TransformationResult<T extends Node> {
        transformed: T[];
        diagnostics?: DiagnosticWithLocation[];
        substituteNode(hint: EmitHint, node: Node): Node;
        emitNodeWithNotification(hint: EmitHint, node: Node, emitCallback: (hint: EmitHint, node: Node) => void): void;
        isEmitNotificationEnabled?(node: Node): boolean;
        dispose(): void;
    }
    type TransformerFactory<T extends Node> = (context: TransformationContext) => Transformer<T>;
    type Transformer<T extends Node> = (node: T) => T;
    type Visitor<TIn extends Node = Node, TOut extends Node | undefined = TIn | undefined> = (node: TIn) => VisitResult<TOut>;
    interface NodeVisitor {
        <TIn extends Node | undefined, TVisited extends Node | undefined, TOut extends Node>(node: TIn, visitor: Visitor<NonNullable<TIn>, TVisited>, test: (node: Node) => node is TOut, lift?: (node: readonly Node[]) => Node): TOut | (TIn & undefined) | (TVisited & undefined);
        <TIn extends Node | undefined, TVisited extends Node | undefined>(node: TIn, visitor: Visitor<NonNullable<TIn>, TVisited>, test?: (node: Node) => boolean, lift?: (node: readonly Node[]) => Node): Node | (TIn & undefined) | (TVisited & undefined);
    }
    interface NodesVisitor {
        <TIn extends Node, TInArray extends NodeArray<TIn> | undefined, TOut extends Node>(nodes: TInArray, visitor: Visitor<TIn, Node | undefined>, test: (node: Node) => node is TOut, start?: number, count?: number): NodeArray<TOut> | (TInArray & undefined);
        <TIn extends Node, TInArray extends NodeArray<TIn> | undefined>(nodes: TInArray, visitor: Visitor<TIn, Node | undefined>, test?: (node: Node) => boolean, start?: number, count?: number): NodeArray<Node> | (TInArray & undefined);
    }
    type VisitResult<T extends Node | undefined> = T | readonly Node[];
    interface Printer {
        printNode(hint: EmitHint, node: Node, sourceFile: SourceFile): string;
        printList<T extends Node>(format: ListFormat, list: NodeArray<T>, sourceFile: SourceFile): string;
        printFile(sourceFile: SourceFile): string;
        printBundle(bundle: Bundle): string;
    }
    interface PrintHandlers {
        hasGlobalName?(name: string): boolean;
        onEmitNode?(hint: EmitHint, node: Node, emitCallback: (hint: EmitHint, node: Node) => void): void;
        isEmitNotificationEnabled?(node: Node): boolean;
        substituteNode?(hint: EmitHint, node: Node): Node;
    }
    interface PrinterOptions {
        removeComments?: boolean;
        newLine?: NewLineKind;
        omitTrailingSemicolon?: boolean;
        noEmitHelpers?: boolean;
    }
    interface GetEffectiveTypeRootsHost {
        getCurrentDirectory?(): string;
    }
    interface TextSpan {
        start: number;
        length: number;
    }
    interface TextChangeRange {
        span: TextSpan;
        newLength: number;
    }
    interface SyntaxList extends Node {
        kind: SyntaxKind.SyntaxList;
    }
    enum ListFormat {
        None = 0,
        SingleLine = 0,
        MultiLine = 1,
        PreserveLines = 2,
        LinesMask = 3,
        NotDelimited = 0,
        BarDelimited = 4,
        AmpersandDelimited = 8,
        CommaDelimited = 16,
        AsteriskDelimited = 32,
        DelimitersMask = 60,
        AllowTrailingComma = 64,
        Indented = 128,
        SpaceBetweenBraces = 256,
        SpaceBetweenSiblings = 512,
        Braces = 1024,
        Parenthesis = 2048,
        AngleBrackets = 4096,
        SquareBrackets = 8192,
        BracketsMask = 15360,
        OptionalIfUndefined = 16384,
        OptionalIfEmpty = 32768,
        Optional = 49152,
        PreferNewLine = 65536,
        NoTrailingNewLine = 131072,
        NoInterveningComments = 262144,
        NoSpaceIfEmpty = 524288,
        SingleElement = 1048576,
        SpaceAfterList = 2097152,
        Modifiers = 2359808,
        HeritageClauses = 512,
        SingleLineTypeLiteralMembers = 768,
        MultiLineTypeLiteralMembers = 32897,
        SingleLineTupleTypeElements = 528,
        MultiLineTupleTypeElements = 657,
        UnionTypeConstituents = 516,
        IntersectionTypeConstituents = 520,
        ObjectBindingPatternElements = 525136,
        ArrayBindingPatternElements = 524880,
        ObjectLiteralExpressionProperties = 526226,
        ImportAttributes = 526226,
ImportClauseEntries = 526226,
        ArrayLiteralExpressionElements = 8914,
        CommaListElements = 528,
        CallExpressionArguments = 2576,
        NewExpressionArguments = 18960,
        TemplateExpressionSpans = 262144,
        SingleLineBlockStatements = 768,
        MultiLineBlockStatements = 129,
        VariableDeclarationList = 528,
        SingleLineFunctionBodyStatements = 768,
        MultiLineFunctionBodyStatements = 1,
        ClassHeritageClauses = 0,
        ClassMembers = 129,
        InterfaceMembers = 129,
        EnumMembers = 145,
        CaseBlockClauses = 129,
        NamedImportsOrExportsElements = 525136,
        JsxElementOrFragmentChildren = 262144,
        JsxElementAttributes = 262656,
        CaseOrDefaultClauseStatements = 163969,
        HeritageClauseTypes = 528,
        SourceFileStatements = 131073,
        Decorators = 2146305,
        TypeArguments = 53776,
        TypeParameters = 53776,
        Parameters = 2576,
        IndexSignatureParameters = 8848,
        JSDocComment = 33,
    }
    enum JSDocParsingMode {
        ParseAll = 0,
        ParseNone = 1,
        ParseForTypeErrors = 2,
        ParseForTypeInfo = 3,
    }
    interface UserPreferences {
        readonly disableSuggestions?: boolean;
        readonly quotePreference?: "auto" | "double" | "single";
        readonly includeCompletionsForModuleExports?: boolean;
        readonly includeCompletionsForImportStatements?: boolean;
        readonly includeCompletionsWithSnippetText?: boolean;
        readonly includeAutomaticOptionalChainCompletions?: boolean;
        readonly includeCompletionsWithInsertText?: boolean;
        readonly includeCompletionsWithClassMemberSnippets?: boolean;
        readonly includeCompletionsWithObjectLiteralMethodSnippets?: boolean;
        readonly useLabelDetailsInCompletionEntries?: boolean;
        readonly allowIncompleteCompletions?: boolean;
        readonly importModuleSpecifierPreference?: "shortest" | "project-relative" | "relative" | "non-relative";
        readonly importModuleSpecifierEnding?: "auto" | "minimal" | "index" | "js";
        readonly allowTextChangesInNewFiles?: boolean;
        readonly providePrefixAndSuffixTextForRename?: boolean;
        readonly includePackageJsonAutoImports?: "auto" | "on" | "off";
        readonly provideRefactorNotApplicableReason?: boolean;
        readonly jsxAttributeCompletionStyle?: "auto" | "braces" | "none";
        readonly includeInlayParameterNameHints?: "none" | "literals" | "all";
        readonly includeInlayParameterNameHintsWhenArgumentMatchesName?: boolean;
        readonly includeInlayFunctionParameterTypeHints?: boolean;
        readonly includeInlayVariableTypeHints?: boolean;
        readonly includeInlayVariableTypeHintsWhenTypeMatchesName?: boolean;
        readonly includeInlayPropertyDeclarationTypeHints?: boolean;
        readonly includeInlayFunctionLikeReturnTypeHints?: boolean;
        readonly includeInlayEnumMemberValueHints?: boolean;
        readonly interactiveInlayHints?: boolean;
        readonly allowRenameOfImportPath?: boolean;
        readonly autoImportFileExcludePatterns?: string[];
        readonly autoImportSpecifierExcludeRegexes?: string[];
        readonly preferTypeOnlyAutoImports?: boolean;
        readonly organizeImportsIgnoreCase?: "auto" | boolean;
        readonly organizeImportsCollation?: "ordinal" | "unicode";
        readonly organizeImportsLocale?: string;
        readonly organizeImportsNumericCollation?: boolean;
        readonly organizeImportsAccentCollation?: boolean;
        readonly organizeImportsCaseFirst?: "upper" | "lower" | false;
        readonly organizeImportsTypeOrder?: OrganizeImportsTypeOrder;
        readonly excludeLibrarySymbolsInNavTo?: boolean;
        readonly lazyConfiguredProjectsFromExternalProject?: boolean;
        readonly displayPartsForJSDoc?: boolean;
        readonly generateReturnInDocTemplate?: boolean;
        readonly disableLineTextInReferences?: boolean;
        readonly maximumHoverLength?: number;
    }
    type OrganizeImportsTypeOrder = "last" | "inline" | "first";
    interface PseudoBigInt {
        negative: boolean;
        base10Value: string;
    }
    enum FileWatcherEventKind {
        Created = 0,
        Changed = 1,
        Deleted = 2,
    }
    type FileWatcherCallback = (fileName: string, eventKind: FileWatcherEventKind, modifiedTime?: Date) => void;
    type DirectoryWatcherCallback = (fileName: string) => void;
    type BufferEncoding = "ascii" | "utf8" | "utf-8" | "utf16le" | "ucs2" | "ucs-2" | "base64" | "latin1" | "binary" | "hex";
    interface System {
        args: string[];
        newLine: string;
        useCaseSensitiveFileNames: boolean;
        write(s: string): void;
        writeOutputIsTTY?(): boolean;
        getWidthOfTerminal?(): number;
        readFile(path: string, encoding?: string): string | undefined;
        getFileSize?(path: string): number;
        writeFile(path: string, data: string, writeByteOrderMark?: boolean): void;
        watchFile?(path: string, callback: FileWatcherCallback, pollingInterval?: number, options?: WatchOptions): FileWatcher;
        watchDirectory?(path: string, callback: DirectoryWatcherCallback, recursive?: boolean, options?: WatchOptions): FileWatcher;
        resolvePath(path: string): string;
        fileExists(path: string): boolean;
        directoryExists(path: string): boolean;
        createDirectory(path: string): void;
        getExecutingFilePath(): string;
        getCurrentDirectory(): string;
        getDirectories(path: string): string[];
        readDirectory(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[];
        getModifiedTime?(path: string): Date | undefined;
        setModifiedTime?(path: string, time: Date): void;
        deleteFile?(path: string): void;
        createHash?(data: string): string;
        createSHA256Hash?(data: string): string;
        getMemoryUsage?(): number;
        exit(exitCode?: number): void;
        realpath?(path: string): string;
        setTimeout?(callback: (...args: any[]) => void, ms: number, ...args: any[]): any;
        clearTimeout?(timeoutId: any): void;
        clearScreen?(): void;
        base64decode?(input: string): string;
        base64encode?(input: string): string;
    }
    interface FileWatcher {
        close(): void;
    }
    let sys: System;
    function tokenToString(t: SyntaxKind): string | undefined;
    function getPositionOfLineAndCharacter(sourceFile: SourceFileLike, line: number, character: number): number;
    function getLineAndCharacterOfPosition(sourceFile: SourceFileLike, position: number): LineAndCharacter;
    function isWhiteSpaceLike(ch: number): boolean;
    function isWhiteSpaceSingleLine(ch: number): boolean;
    function isLineBreak(ch: number): boolean;
    function couldStartTrivia(text: string, pos: number): boolean;
    function forEachLeadingCommentRange<U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean) => U): U | undefined;
    function forEachLeadingCommentRange<T, U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean, state: T) => U, state: T): U | undefined;
    function forEachTrailingCommentRange<U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean) => U): U | undefined;
    function forEachTrailingCommentRange<T, U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean, state: T) => U, state: T): U | undefined;
    function reduceEachLeadingCommentRange<T, U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean, state: T) => U, state: T, initial: U): U | undefined;
    function reduceEachTrailingCommentRange<T, U>(text: string, pos: number, cb: (pos: number, end: number, kind: CommentKind, hasTrailingNewLine: boolean, state: T) => U, state: T, initial: U): U | undefined;
    function getLeadingCommentRanges(text: string, pos: number): CommentRange[] | undefined;
    function getTrailingCommentRanges(text: string, pos: number): CommentRange[] | undefined;
    function getShebang(text: string): string | undefined;
    function isIdentifierStart(ch: number, languageVersion: ScriptTarget | undefined): boolean;
    function isIdentifierPart(ch: number, languageVersion: ScriptTarget | undefined, identifierVariant?: LanguageVariant): boolean;
    function createScanner(languageVersion: ScriptTarget, skipTrivia: boolean, languageVariant?: LanguageVariant, textInitial?: string, onError?: ErrorCallback, start?: number, length?: number): Scanner;
    type ErrorCallback = (message: DiagnosticMessage, length: number, arg0?: any) => void;
    interface Scanner {
        getStartPos(): number;
        getToken(): SyntaxKind;
        getTokenFullStart(): number;
        getTokenStart(): number;
        getTokenEnd(): number;
        getTextPos(): number;
        getTokenPos(): number;
        getTokenText(): string;
        getTokenValue(): string;
        hasUnicodeEscape(): boolean;
        hasExtendedUnicodeEscape(): boolean;
        hasPrecedingLineBreak(): boolean;
        isIdentifier(): boolean;
        isReservedWord(): boolean;
        isUnterminated(): boolean;
        reScanGreaterToken(): SyntaxKind;
        reScanSlashToken(): SyntaxKind;
        reScanAsteriskEqualsToken(): SyntaxKind;
        reScanTemplateToken(isTaggedTemplate: boolean): SyntaxKind;
        reScanTemplateHeadOrNoSubstitutionTemplate(): SyntaxKind;
        scanJsxIdentifier(): SyntaxKind;
        scanJsxAttributeValue(): SyntaxKind;
        reScanJsxAttributeValue(): SyntaxKind;
        reScanJsxToken(allowMultilineJsxText?: boolean): JsxTokenSyntaxKind;
        reScanLessThanToken(): SyntaxKind;
        reScanHashToken(): SyntaxKind;
        reScanQuestionToken(): SyntaxKind;
        reScanInvalidIdentifier(): SyntaxKind;
        scanJsxToken(): JsxTokenSyntaxKind;
        scanJsDocToken(): JSDocSyntaxKind;
        scan(): SyntaxKind;
        getText(): string;
        setText(text: string | undefined, start?: number, length?: number): void;
        setOnError(onError: ErrorCallback | undefined): void;
        setScriptTarget(scriptTarget: ScriptTarget): void;
        setLanguageVariant(variant: LanguageVariant): void;
        setScriptKind(scriptKind: ScriptKind): void;
        setJSDocParsingMode(kind: JSDocParsingMode): void;
        setTextPos(textPos: number): void;
        resetTokenState(pos: number): void;
        lookAhead<T>(callback: () => T): T;
        scanRange<T>(start: number, length: number, callback: () => T): T;
        tryScan<T>(callback: () => T): T;
    }
    function isExternalModuleNameRelative(moduleName: string): boolean;
    function sortAndDeduplicateDiagnostics<T extends Diagnostic>(diagnostics: readonly T[]): SortedReadonlyArray<T>;
    function getDefaultLibFileName(options: CompilerOptions): string;
    function textSpanEnd(span: TextSpan): number;
    function textSpanIsEmpty(span: TextSpan): boolean;
    function textSpanContainsPosition(span: TextSpan, position: number): boolean;
    function textSpanContainsTextSpan(span: TextSpan, other: TextSpan): boolean;
    function textSpanOverlapsWith(span: TextSpan, other: TextSpan): boolean;
    function textSpanOverlap(span1: TextSpan, span2: TextSpan): TextSpan | undefined;
    function textSpanIntersectsWithTextSpan(span: TextSpan, other: TextSpan): boolean;
    function textSpanIntersectsWith(span: TextSpan, start: number, length: number): boolean;
    function decodedTextSpanIntersectsWith(start1: number, length1: number, start2: number, length2: number): boolean;
    function textSpanIntersectsWithPosition(span: TextSpan, position: number): boolean;
    function textSpanIntersection(span1: TextSpan, span2: TextSpan): TextSpan | undefined;
    function createTextSpan(start: number, length: number): TextSpan;
    function createTextSpanFromBounds(start: number, end: number): TextSpan;
    function textChangeRangeNewSpan(range: TextChangeRange): TextSpan;
    function textChangeRangeIsUnchanged(range: TextChangeRange): boolean;
    function createTextChangeRange(span: TextSpan, newLength: number): TextChangeRange;
    function collapseTextChangeRangesAcrossMultipleVersions(changes: readonly TextChangeRange[]): TextChangeRange;
    function getTypeParameterOwner(d: Declaration): Declaration | undefined;
    function isParameterPropertyDeclaration(node: Node, parent: Node): node is ParameterPropertyDeclaration;
    function isEmptyBindingPattern(node: BindingName): node is BindingPattern;
    function isEmptyBindingElement(node: BindingElement | ArrayBindingElement): boolean;
    function walkUpBindingElementsAndPatterns(binding: BindingElement): VariableDeclaration | ParameterDeclaration;
    function getCombinedModifierFlags(node: Declaration): ModifierFlags;
    function getCombinedNodeFlags(node: Node): NodeFlags;
    function validateLocaleAndSetLanguage(locale: string, sys: {
        getExecutingFilePath(): string;
        resolvePath(path: string): string;
        fileExists(fileName: string): boolean;
        readFile(fileName: string): string | undefined;
    }, errors?: Diagnostic[]): void;
    function getOriginalNode(node: Node): Node;
    function getOriginalNode<T extends Node>(node: Node, nodeTest: (node: Node) => node is T): T;
    function getOriginalNode(node: Node | undefined): Node | undefined;
    function getOriginalNode<T extends Node>(node: Node | undefined, nodeTest: (node: Node) => node is T): T | undefined;
    function findAncestor<T extends Node>(node: Node | undefined, callback: (element: Node) => element is T): T | undefined;
    function findAncestor(node: Node | undefined, callback: (element: Node) => boolean | "quit"): Node | undefined;
    function isParseTreeNode(node: Node): boolean;
    function getParseTreeNode(node: Node | undefined): Node | undefined;
    function getParseTreeNode<T extends Node>(node: T | undefined, nodeTest?: (node: Node) => node is T): T | undefined;
    function escapeLeadingUnderscores(identifier: string): __String;
    function unescapeLeadingUnderscores(identifier: __String): string;
    function idText(identifierOrPrivateName: Identifier | PrivateIdentifier): string;
    function identifierToKeywordKind(node: Identifier): KeywordSyntaxKind | undefined;
    function symbolName(symbol: Symbol): string;
    function getNameOfJSDocTypedef(declaration: JSDocTypedefTag): Identifier | PrivateIdentifier | undefined;
    function getNameOfDeclaration(declaration: Declaration | Expression | undefined): DeclarationName | undefined;
    function getDecorators(node: HasDecorators): readonly Decorator[] | undefined;
    function getModifiers(node: HasModifiers): readonly Modifier[] | undefined;
    function getJSDocParameterTags(param: ParameterDeclaration): readonly JSDocParameterTag[];
    function getJSDocTypeParameterTags(param: TypeParameterDeclaration): readonly JSDocTemplateTag[];
    function hasJSDocParameterTags(node: FunctionLikeDeclaration | SignatureDeclaration): boolean;
    function getJSDocAugmentsTag(node: Node): JSDocAugmentsTag | undefined;
    function getJSDocImplementsTags(node: Node): readonly JSDocImplementsTag[];
    function getJSDocClassTag(node: Node): JSDocClassTag | undefined;
    function getJSDocPublicTag(node: Node): JSDocPublicTag | undefined;
    function getJSDocPrivateTag(node: Node): JSDocPrivateTag | undefined;
    function getJSDocProtectedTag(node: Node): JSDocProtectedTag | undefined;
    function getJSDocReadonlyTag(node: Node): JSDocReadonlyTag | undefined;
    function getJSDocOverrideTagNoCache(node: Node): JSDocOverrideTag | undefined;
    function getJSDocDeprecatedTag(node: Node): JSDocDeprecatedTag | undefined;
    function getJSDocEnumTag(node: Node): JSDocEnumTag | undefined;
    function getJSDocThisTag(node: Node): JSDocThisTag | undefined;
    function getJSDocReturnTag(node: Node): JSDocReturnTag | undefined;
    function getJSDocTemplateTag(node: Node): JSDocTemplateTag | undefined;
    function getJSDocSatisfiesTag(node: Node): JSDocSatisfiesTag | undefined;
    function getJSDocTypeTag(node: Node): JSDocTypeTag | undefined;
    function getJSDocType(node: Node): TypeNode | undefined;
    function getJSDocReturnType(node: Node): TypeNode | undefined;
    function getJSDocTags(node: Node): readonly JSDocTag[];
    function getAllJSDocTags<T extends JSDocTag>(node: Node, predicate: (tag: JSDocTag) => tag is T): readonly T[];
    function getAllJSDocTagsOfKind(node: Node, kind: SyntaxKind): readonly JSDocTag[];
    function getTextOfJSDocComment(comment?: string | NodeArray<JSDocComment>): string | undefined;
    function getEffectiveTypeParameterDeclarations(node: DeclarationWithTypeParameters): readonly TypeParameterDeclaration[];
    function getEffectiveConstraintOfTypeParameter(node: TypeParameterDeclaration): TypeNode | undefined;
    function isMemberName(node: Node): node is MemberName;
    function isPropertyAccessChain(node: Node): node is PropertyAccessChain;
    function isElementAccessChain(node: Node): node is ElementAccessChain;
    function isCallChain(node: Node): node is CallChain;
    function isOptionalChain(node: Node): node is PropertyAccessChain | ElementAccessChain | CallChain | NonNullChain;
    function isNullishCoalesce(node: Node): boolean;
    function isConstTypeReference(node: Node): boolean;
    function skipPartiallyEmittedExpressions(node: Expression): Expression;
    function skipPartiallyEmittedExpressions(node: Node): Node;
    function isNonNullChain(node: Node): node is NonNullChain;
    function isBreakOrContinueStatement(node: Node): node is BreakOrContinueStatement;
    function isNamedExportBindings(node: Node): node is NamedExportBindings;
    function isJSDocPropertyLikeTag(node: Node): node is JSDocPropertyLikeTag;
    function isTokenKind(kind: SyntaxKind): boolean;
    function isToken(n: Node): boolean;
    function isLiteralExpression(node: Node): node is LiteralExpression;
    function isTemplateLiteralToken(node: Node): node is TemplateLiteralToken;
    function isTemplateMiddleOrTemplateTail(node: Node): node is TemplateMiddle | TemplateTail;
    function isImportOrExportSpecifier(node: Node): node is ImportSpecifier | ExportSpecifier;
    function isTypeOnlyImportDeclaration(node: Node): node is TypeOnlyImportDeclaration;
    function isTypeOnlyExportDeclaration(node: Node): node is TypeOnlyExportDeclaration;
    function isTypeOnlyImportOrExportDeclaration(node: Node): node is TypeOnlyAliasDeclaration;
    function isPartOfTypeOnlyImportOrExportDeclaration(node: Node): boolean;
    function isStringTextContainingNode(node: Node): node is StringLiteral | TemplateLiteralToken;
    function isImportAttributeName(node: Node): node is ImportAttributeName;
    function isModifier(node: Node): node is Modifier;
    function isEntityName(node: Node): node is EntityName;
    function isPropertyName(node: Node): node is PropertyName;
    function isBindingName(node: Node): node is BindingName;
    function isFunctionLike(node: Node | undefined): node is SignatureDeclaration;
    function isClassElement(node: Node): node is ClassElement;
    function isClassLike(node: Node): node is ClassLikeDeclaration;
    function isAccessor(node: Node): node is AccessorDeclaration;
    function isAutoAccessorPropertyDeclaration(node: Node): node is AutoAccessorPropertyDeclaration;
    function isModifierLike(node: Node): node is ModifierLike;
    function isTypeElement(node: Node): node is TypeElement;
    function isClassOrTypeElement(node: Node): node is ClassElement | TypeElement;
    function isObjectLiteralElementLike(node: Node): node is ObjectLiteralElementLike;
    function isTypeNode(node: Node): node is TypeNode;
    function isFunctionOrConstructorTypeNode(node: Node): node is FunctionTypeNode | ConstructorTypeNode;
    function isArrayBindingElement(node: Node): node is ArrayBindingElement;
    function isPropertyAccessOrQualifiedName(node: Node): node is PropertyAccessExpression | QualifiedName;
    function isCallLikeExpression(node: Node): node is CallLikeExpression;
    function isCallOrNewExpression(node: Node): node is CallExpression | NewExpression;
    function isTemplateLiteral(node: Node): node is TemplateLiteral;
    function isLeftHandSideExpression(node: Node): node is LeftHandSideExpression;
    function isLiteralTypeLiteral(node: Node): node is NullLiteral | BooleanLiteral | LiteralExpression | PrefixUnaryExpression;
    function isExpression(node: Node): node is Expression;
    function isAssertionExpression(node: Node): node is AssertionExpression;
    function isIterationStatement(node: Node, lookInLabeledStatements: false): node is IterationStatement;
    function isIterationStatement(node: Node, lookInLabeledStatements: boolean): node is IterationStatement | LabeledStatement;
    function isConciseBody(node: Node): node is ConciseBody;
    function isForInitializer(node: Node): node is ForInitializer;
    function isModuleBody(node: Node): node is ModuleBody;
    function isNamedImportBindings(node: Node): node is NamedImportBindings;
    function isDeclarationStatement(node: Node): node is DeclarationStatement;
    function isStatement(node: Node): node is Statement;
    function isModuleReference(node: Node): node is ModuleReference;
    function isJsxTagNameExpression(node: Node): node is JsxTagNameExpression;
    function isJsxChild(node: Node): node is JsxChild;
    function isJsxAttributeLike(node: Node): node is JsxAttributeLike;
    function isStringLiteralOrJsxExpression(node: Node): node is StringLiteral | JsxExpression;
    function isJsxOpeningLikeElement(node: Node): node is JsxOpeningLikeElement;
    function isJsxCallLike(node: Node): node is JsxCallLike;
    function isCaseOrDefaultClause(node: Node): node is CaseOrDefaultClause;
    function isJSDocCommentContainingNode(node: Node): boolean;
    function isSetAccessor(node: Node): node is SetAccessorDeclaration;
    function isGetAccessor(node: Node): node is GetAccessorDeclaration;
    function hasOnlyExpressionInitializer(node: Node): node is HasExpressionInitializer;
    function isObjectLiteralElement(node: Node): node is ObjectLiteralElement;
    function isStringLiteralLike(node: Node | FileReference): node is StringLiteralLike;
    function isJSDocLinkLike(node: Node): node is JSDocLink | JSDocLinkCode | JSDocLinkPlain;
    function hasRestParameter(s: SignatureDeclaration | JSDocSignature): boolean;
    function isRestParameter(node: ParameterDeclaration | JSDocParameterTag): boolean;
    function isInternalDeclaration(node: Node, sourceFile?: SourceFile): boolean;
    const unchangedTextChangeRange: TextChangeRange;
    type ParameterPropertyDeclaration = ParameterDeclaration & {
        parent: ConstructorDeclaration;
        name: Identifier;
    };
    function isPartOfTypeNode(node: Node): boolean;
    function getJSDocCommentsAndTags(hostNode: Node): readonly (JSDoc | JSDocTag)[];
    function createSourceMapSource(fileName: string, text: string, skipTrivia?: (pos: number) => number): SourceMapSource;
    function setOriginalNode<T extends Node>(node: T, original: Node | undefined): T;
    const factory: NodeFactory;
    function disposeEmitNodes(sourceFile: SourceFile | undefined): void;
    function setEmitFlags<T extends Node>(node: T, emitFlags: EmitFlags): T;
    function getSourceMapRange(node: Node): SourceMapRange;
    function setSourceMapRange<T extends Node>(node: T, range: SourceMapRange | undefined): T;
    function getTokenSourceMapRange(node: Node, token: SyntaxKind): SourceMapRange | undefined;
    function setTokenSourceMapRange<T extends Node>(node: T, token: SyntaxKind, range: SourceMapRange | undefined): T;
    function getCommentRange(node: Node): TextRange;
    function setCommentRange<T extends Node>(node: T, range: TextRange): T;
    function getSyntheticLeadingComments(node: Node): SynthesizedComment[] | undefined;
    function setSyntheticLeadingComments<T extends Node>(node: T, comments: SynthesizedComment[] | undefined): T;
    function addSyntheticLeadingComment<T extends Node>(node: T, kind: SyntaxKind.SingleLineCommentTrivia | SyntaxKind.MultiLineCommentTrivia, text: string, hasTrailingNewLine?: boolean): T;
    function getSyntheticTrailingComments(node: Node): SynthesizedComment[] | undefined;
    function setSyntheticTrailingComments<T extends Node>(node: T, comments: SynthesizedComment[] | undefined): T;
    function addSyntheticTrailingComment<T extends Node>(node: T, kind: SyntaxKind.SingleLineCommentTrivia | SyntaxKind.MultiLineCommentTrivia, text: string, hasTrailingNewLine?: boolean): T;
    function moveSyntheticComments<T extends Node>(node: T, original: Node): T;
    function getConstantValue(node: AccessExpression): string | number | undefined;
    function setConstantValue(node: AccessExpression, value: string | number): AccessExpression;
    function addEmitHelper<T extends Node>(node: T, helper: EmitHelper): T;
    function addEmitHelpers<T extends Node>(node: T, helpers: EmitHelper[] | undefined): T;
    function removeEmitHelper(node: Node, helper: EmitHelper): boolean;
    function getEmitHelpers(node: Node): EmitHelper[] | undefined;
    function moveEmitHelpers(source: Node, target: Node, predicate: (helper: EmitHelper) => boolean): void;
    function isNumericLiteral(node: Node): node is NumericLiteral;
    function isBigIntLiteral(node: Node): node is BigIntLiteral;
    function isStringLiteral(node: Node): node is StringLiteral;
    function isJsxText(node: Node): node is JsxText;
    function isRegularExpressionLiteral(node: Node): node is RegularExpressionLiteral;
    function isNoSubstitutionTemplateLiteral(node: Node): node is NoSubstitutionTemplateLiteral;
    function isTemplateHead(node: Node): node is TemplateHead;
    function isTemplateMiddle(node: Node): node is TemplateMiddle;
    function isTemplateTail(node: Node): node is TemplateTail;
    function isDotDotDotToken(node: Node): node is DotDotDotToken;
    function isPlusToken(node: Node): node is PlusToken;
    function isMinusToken(node: Node): node is MinusToken;
    function isAsteriskToken(node: Node): node is AsteriskToken;
    function isExclamationToken(node: Node): node is ExclamationToken;
    function isQuestionToken(node: Node): node is QuestionToken;
    function isColonToken(node: Node): node is ColonToken;
    function isQuestionDotToken(node: Node): node is QuestionDotToken;
    function isEqualsGreaterThanToken(node: Node): node is EqualsGreaterThanToken;
    function isIdentifier(node: Node): node is Identifier;
    function isPrivateIdentifier(node: Node): node is PrivateIdentifier;
    function isAssertsKeyword(node: Node): node is AssertsKeyword;
    function isAwaitKeyword(node: Node): node is AwaitKeyword;
    function isQualifiedName(node: Node): node is QualifiedName;
    function isComputedPropertyName(node: Node): node is ComputedPropertyName;
    function isTypeParameterDeclaration(node: Node): node is TypeParameterDeclaration;
    function isParameter(node: Node): node is ParameterDeclaration;
    function isDecorator(node: Node): node is Decorator;
    function isPropertySignature(node: Node): node is PropertySignature;
    function isPropertyDeclaration(node: Node): node is PropertyDeclaration;
    function isMethodSignature(node: Node): node is MethodSignature;
    function isMethodDeclaration(node: Node): node is MethodDeclaration;
    function isClassStaticBlockDeclaration(node: Node): node is ClassStaticBlockDeclaration;
    function isConstructorDeclaration(node: Node): node is ConstructorDeclaration;
    function isGetAccessorDeclaration(node: Node): node is GetAccessorDeclaration;
    function isSetAccessorDeclaration(node: Node): node is SetAccessorDeclaration;
    function isCallSignatureDeclaration(node: Node): node is CallSignatureDeclaration;
    function isConstructSignatureDeclaration(node: Node): node is ConstructSignatureDeclaration;
    function isIndexSignatureDeclaration(node: Node): node is IndexSignatureDeclaration;
    function isTypePredicateNode(node: Node): node is TypePredicateNode;
    function isTypeReferenceNode(node: Node): node is TypeReferenceNode;
    function isFunctionTypeNode(node: Node): node is FunctionTypeNode;
    function isConstructorTypeNode(node: Node): node is ConstructorTypeNode;
    function isTypeQueryNode(node: Node): node is TypeQueryNode;
    function isTypeLiteralNode(node: Node): node is TypeLiteralNode;
    function isArrayTypeNode(node: Node): node is ArrayTypeNode;
    function isTupleTypeNode(node: Node): node is TupleTypeNode;
    function isNamedTupleMember(node: Node): node is NamedTupleMember;
    function isOptionalTypeNode(node: Node): node is OptionalTypeNode;
    function isRestTypeNode(node: Node): node is RestTypeNode;
    function isUnionTypeNode(node: Node): node is UnionTypeNode;
    function isIntersectionTypeNode(node: Node): node is IntersectionTypeNode;
    function isConditionalTypeNode(node: Node): node is ConditionalTypeNode;
    function isInferTypeNode(node: Node): node is InferTypeNode;
    function isParenthesizedTypeNode(node: Node): node is ParenthesizedTypeNode;
    function isThisTypeNode(node: Node): node is ThisTypeNode;
    function isTypeOperatorNode(node: Node): node is TypeOperatorNode;
    function isIndexedAccessTypeNode(node: Node): node is IndexedAccessTypeNode;
    function isMappedTypeNode(node: Node): node is MappedTypeNode;
    function isLiteralTypeNode(node: Node): node is LiteralTypeNode;
    function isImportTypeNode(node: Node): node is ImportTypeNode;
    function isTemplateLiteralTypeSpan(node: Node): node is TemplateLiteralTypeSpan;
    function isTemplateLiteralTypeNode(node: Node): node is TemplateLiteralTypeNode;
    function isObjectBindingPattern(node: Node): node is ObjectBindingPattern;
    function isArrayBindingPattern(node: Node): node is ArrayBindingPattern;
    function isBindingElement(node: Node): node is BindingElement;
    function isArrayLiteralExpression(node: Node): node is ArrayLiteralExpression;
    function isObjectLiteralExpression(node: Node): node is ObjectLiteralExpression;
    function isPropertyAccessExpression(node: Node): node is PropertyAccessExpression;
    function isElementAccessExpression(node: Node): node is ElementAccessExpression;
    function isCallExpression(node: Node): node is CallExpression;
    function isNewExpression(node: Node): node is NewExpression;
    function isTaggedTemplateExpression(node: Node): node is TaggedTemplateExpression;
    function isTypeAssertionExpression(node: Node): node is TypeAssertion;
    function isParenthesizedExpression(node: Node): node is ParenthesizedExpression;
    function isFunctionExpression(node: Node): node is FunctionExpression;
    function isArrowFunction(node: Node): node is ArrowFunction;
    function isDeleteExpression(node: Node): node is DeleteExpression;
    function isTypeOfExpression(node: Node): node is TypeOfExpression;
    function isVoidExpression(node: Node): node is VoidExpression;
    function isAwaitExpression(node: Node): node is AwaitExpression;
    function isPrefixUnaryExpression(node: Node): node is PrefixUnaryExpression;
    function isPostfixUnaryExpression(node: Node): node is PostfixUnaryExpression;
    function isBinaryExpression(node: Node): node is BinaryExpression;
    function isConditionalExpression(node: Node): node is ConditionalExpression;
    function isTemplateExpression(node: Node): node is TemplateExpression;
    function isYieldExpression(node: Node): node is YieldExpression;
    function isSpreadElement(node: Node): node is SpreadElement;
    function isClassExpression(node: Node): node is ClassExpression;
    function isOmittedExpression(node: Node): node is OmittedExpression;
    function isExpressionWithTypeArguments(node: Node): node is ExpressionWithTypeArguments;
    function isAsExpression(node: Node): node is AsExpression;
    function isSatisfiesExpression(node: Node): node is SatisfiesExpression;
    function isNonNullExpression(node: Node): node is NonNullExpression;
    function isMetaProperty(node: Node): node is MetaProperty;
    function isSyntheticExpression(node: Node): node is SyntheticExpression;
    function isPartiallyEmittedExpression(node: Node): node is PartiallyEmittedExpression;
    function isCommaListExpression(node: Node): node is CommaListExpression;
    function isTemplateSpan(node: Node): node is TemplateSpan;
    function isSemicolonClassElement(node: Node): node is SemicolonClassElement;
    function isBlock(node: Node): node is Block;
    function isVariableStatement(node: Node): node is VariableStatement;
    function isEmptyStatement(node: Node): node is EmptyStatement;
    function isExpressionStatement(node: Node): node is ExpressionStatement;
    function isIfStatement(node: Node): node is IfStatement;
    function isDoStatement(node: Node): node is DoStatement;
    function isWhileStatement(node: Node): node is WhileStatement;
    function isForStatement(node: Node): node is ForStatement;
    function isForInStatement(node: Node): node is ForInStatement;
    function isForOfStatement(node: Node): node is ForOfStatement;
    function isContinueStatement(node: Node): node is ContinueStatement;
    function isBreakStatement(node: Node): node is BreakStatement;
    function isReturnStatement(node: Node): node is ReturnStatement;
    function isWithStatement(node: Node): node is WithStatement;
    function isSwitchStatement(node: Node): node is SwitchStatement;
    function isLabeledStatement(node: Node): node is LabeledStatement;
    function isThrowStatement(node: Node): node is ThrowStatement;
    function isTryStatement(node: Node): node is TryStatement;
    function isDebuggerStatement(node: Node): node is DebuggerStatement;
    function isVariableDeclaration(node: Node): node is VariableDeclaration;
    function isVariableDeclarationList(node: Node): node is VariableDeclarationList;
    function isFunctionDeclaration(node: Node): node is FunctionDeclaration;
    function isClassDeclaration(node: Node): node is ClassDeclaration;
    function isInterfaceDeclaration(node: Node): node is InterfaceDeclaration;
    function isTypeAliasDeclaration(node: Node): node is TypeAliasDeclaration;
    function isEnumDeclaration(node: Node): node is EnumDeclaration;
    function isModuleDeclaration(node: Node): node is ModuleDeclaration;
    function isModuleBlock(node: Node): node is ModuleBlock;
    function isCaseBlock(node: Node): node is CaseBlock;
    function isNamespaceExportDeclaration(node: Node): node is NamespaceExportDeclaration;
    function isImportEqualsDeclaration(node: Node): node is ImportEqualsDeclaration;
    function isImportDeclaration(node: Node): node is ImportDeclaration;
    function isImportClause(node: Node): node is ImportClause;
    function isImportTypeAssertionContainer(node: Node): node is ImportTypeAssertionContainer;
    function isAssertClause(node: Node): node is AssertClause;
    function isAssertEntry(node: Node): node is AssertEntry;
    function isImportAttributes(node: Node): node is ImportAttributes;
    function isImportAttribute(node: Node): node is ImportAttribute;
    function isNamespaceImport(node: Node): node is NamespaceImport;
    function isNamespaceExport(node: Node): node is NamespaceExport;
    function isNamedImports(node: Node): node is NamedImports;
    function isImportSpecifier(node: Node): node is ImportSpecifier;
    function isExportAssignment(node: Node): node is ExportAssignment;
    function isExportDeclaration(node: Node): node is ExportDeclaration;
    function isNamedExports(node: Node): node is NamedExports;
    function isExportSpecifier(node: Node): node is ExportSpecifier;
    function isModuleExportName(node: Node): node is ModuleExportName;
    function isMissingDeclaration(node: Node): node is MissingDeclaration;
    function isNotEmittedStatement(node: Node): node is NotEmittedStatement;
    function isExternalModuleReference(node: Node): node is ExternalModuleReference;
    function isJsxElement(node: Node): node is JsxElement;
    function isJsxSelfClosingElement(node: Node): node is JsxSelfClosingElement;
    function isJsxOpeningElement(node: Node): node is JsxOpeningElement;
    function isJsxClosingElement(node: Node): node is JsxClosingElement;
    function isJsxFragment(node: Node): node is JsxFragment;
    function isJsxOpeningFragment(node: Node): node is JsxOpeningFragment;
    function isJsxClosingFragment(node: Node): node is JsxClosingFragment;
    function isJsxAttribute(node: Node): node is JsxAttribute;
    function isJsxAttributes(node: Node): node is JsxAttributes;
    function isJsxSpreadAttribute(node: Node): node is JsxSpreadAttribute;
    function isJsxExpression(node: Node): node is JsxExpression;
    function isJsxNamespacedName(node: Node): node is JsxNamespacedName;
    function isCaseClause(node: Node): node is CaseClause;
    function isDefaultClause(node: Node): node is DefaultClause;
    function isHeritageClause(node: Node): node is HeritageClause;
    function isCatchClause(node: Node): node is CatchClause;
    function isPropertyAssignment(node: Node): node is PropertyAssignment;
    function isShorthandPropertyAssignment(node: Node): node is ShorthandPropertyAssignment;
    function isSpreadAssignment(node: Node): node is SpreadAssignment;
    function isEnumMember(node: Node): node is EnumMember;
    function isSourceFile(node: Node): node is SourceFile;
    function isBundle(node: Node): node is Bundle;
    function isJSDocTypeExpression(node: Node): node is JSDocTypeExpression;
    function isJSDocNameReference(node: Node): node is JSDocNameReference;
    function isJSDocMemberName(node: Node): node is JSDocMemberName;
    function isJSDocLink(node: Node): node is JSDocLink;
    function isJSDocLinkCode(node: Node): node is JSDocLinkCode;
    function isJSDocLinkPlain(node: Node): node is JSDocLinkPlain;
    function isJSDocAllType(node: Node): node is JSDocAllType;
    function isJSDocUnknownType(node: Node): node is JSDocUnknownType;
    function isJSDocNullableType(node: Node): node is JSDocNullableType;
    function isJSDocNonNullableType(node: Node): node is JSDocNonNullableType;
    function isJSDocOptionalType(node: Node): node is JSDocOptionalType;
    function isJSDocFunctionType(node: Node): node is JSDocFunctionType;
    function isJSDocVariadicType(node: Node): node is JSDocVariadicType;
    function isJSDocNamepathType(node: Node): node is JSDocNamepathType;
    function isJSDoc(node: Node): node is JSDoc;
    function isJSDocTypeLiteral(node: Node): node is JSDocTypeLiteral;
    function isJSDocSignature(node: Node): node is JSDocSignature;
    function isJSDocAugmentsTag(node: Node): node is JSDocAugmentsTag;
    function isJSDocAuthorTag(node: Node): node is JSDocAuthorTag;
    function isJSDocClassTag(node: Node): node is JSDocClassTag;
    function isJSDocCallbackTag(node: Node): node is JSDocCallbackTag;
    function isJSDocPublicTag(node: Node): node is JSDocPublicTag;
    function isJSDocPrivateTag(node: Node): node is JSDocPrivateTag;
    function isJSDocProtectedTag(node: Node): node is JSDocProtectedTag;
    function isJSDocReadonlyTag(node: Node): node is JSDocReadonlyTag;
    function isJSDocOverrideTag(node: Node): node is JSDocOverrideTag;
    function isJSDocOverloadTag(node: Node): node is JSDocOverloadTag;
    function isJSDocDeprecatedTag(node: Node): node is JSDocDeprecatedTag;
    function isJSDocSeeTag(node: Node): node is JSDocSeeTag;
    function isJSDocEnumTag(node: Node): node is JSDocEnumTag;
    function isJSDocParameterTag(node: Node): node is JSDocParameterTag;
    function isJSDocReturnTag(node: Node): node is JSDocReturnTag;
    function isJSDocThisTag(node: Node): node is JSDocThisTag;
    function isJSDocTypeTag(node: Node): node is JSDocTypeTag;
    function isJSDocTemplateTag(node: Node): node is JSDocTemplateTag;
    function isJSDocTypedefTag(node: Node): node is JSDocTypedefTag;
    function isJSDocUnknownTag(node: Node): node is JSDocUnknownTag;
    function isJSDocPropertyTag(node: Node): node is JSDocPropertyTag;
    function isJSDocImplementsTag(node: Node): node is JSDocImplementsTag;
    function isJSDocSatisfiesTag(node: Node): node is JSDocSatisfiesTag;
    function isJSDocThrowsTag(node: Node): node is JSDocThrowsTag;
    function isJSDocImportTag(node: Node): node is JSDocImportTag;
    function isQuestionOrExclamationToken(node: Node): node is QuestionToken | ExclamationToken;
    function isIdentifierOrThisTypeNode(node: Node): node is Identifier | ThisTypeNode;
    function isReadonlyKeywordOrPlusOrMinusToken(node: Node): node is ReadonlyKeyword | PlusToken | MinusToken;
    function isQuestionOrPlusOrMinusToken(node: Node): node is QuestionToken | PlusToken | MinusToken;
    function isModuleName(node: Node): node is ModuleName;
    function isBinaryOperatorToken(node: Node): node is BinaryOperatorToken;
    function setTextRange<T extends TextRange>(range: T, location: TextRange | undefined): T;
    function canHaveModifiers(node: Node): node is HasModifiers;
    function canHaveDecorators(node: Node): node is HasDecorators;
    function forEachChild<T>(node: Node, cbNode: (node: Node) => T | undefined, cbNodes?: (nodes: NodeArray<Node>) => T | undefined): T | undefined;
    function createSourceFile(fileName: string, sourceText: string, languageVersionOrOptions: ScriptTarget | CreateSourceFileOptions, setParentNodes?: boolean, scriptKind?: ScriptKind): SourceFile;
    function parseIsolatedEntityName(text: string, languageVersion: ScriptTarget): EntityName | undefined;
    function parseJsonText(fileName: string, sourceText: string): JsonSourceFile;
    function isExternalModule(file: SourceFile): boolean;
    function updateSourceFile(sourceFile: SourceFile, newText: string, textChangeRange: TextChangeRange, aggressiveChecks?: boolean): SourceFile;
    interface CreateSourceFileOptions {
        languageVersion: ScriptTarget;
        impliedNodeFormat?: ResolutionMode;
        setExternalModuleIndicator?: (file: SourceFile) => void;
        jsDocParsingMode?: JSDocParsingMode;
    }
    function parseCommandLine(commandLine: readonly string[], readFile?: (path: string) => string | undefined): ParsedCommandLine;
    function parseBuildCommand(commandLine: readonly string[]): ParsedBuildCommand;
    function getParsedCommandLineOfConfigFile(configFileName: string, optionsToExtend: CompilerOptions | undefined, host: ParseConfigFileHost, extendedConfigCache?: Map<string, ExtendedConfigCacheEntry>, watchOptionsToExtend?: WatchOptions, extraFileExtensions?: readonly FileExtensionInfo[]): ParsedCommandLine | undefined;
    function readConfigFile(fileName: string, readFile: (path: string) => string | undefined): {
        config?: any;
        error?: Diagnostic;
    };
    function parseConfigFileTextToJson(fileName: string, jsonText: string): {
        config?: any;
        error?: Diagnostic;
    };
    function readJsonConfigFile(fileName: string, readFile: (path: string) => string | undefined): TsConfigSourceFile;
    function convertToObject(sourceFile: JsonSourceFile, errors: Diagnostic[]): any;
    function parseJsonConfigFileContent(json: any, host: ParseConfigHost, basePath: string, existingOptions?: CompilerOptions, configFileName?: string, resolutionStack?: Path[], extraFileExtensions?: readonly FileExtensionInfo[], extendedConfigCache?: Map<string, ExtendedConfigCacheEntry>, existingWatchOptions?: WatchOptions): ParsedCommandLine;
    function parseJsonSourceFileConfigFileContent(sourceFile: TsConfigSourceFile, host: ParseConfigHost, basePath: string, existingOptions?: CompilerOptions, configFileName?: string, resolutionStack?: Path[], extraFileExtensions?: readonly FileExtensionInfo[], extendedConfigCache?: Map<string, ExtendedConfigCacheEntry>, existingWatchOptions?: WatchOptions): ParsedCommandLine;
    function convertCompilerOptionsFromJson(jsonOptions: any, basePath: string, configFileName?: string): {
        options: CompilerOptions;
        errors: Diagnostic[];
    };
    function convertTypeAcquisitionFromJson(jsonOptions: any, basePath: string, configFileName?: string): {
        options: TypeAcquisition;
        errors: Diagnostic[];
    };
    interface ParsedBuildCommand {
        buildOptions: BuildOptions;
        watchOptions: WatchOptions | undefined;
        projects: string[];
        errors: Diagnostic[];
    }
    type DiagnosticReporter = (diagnostic: Diagnostic) => void;
    interface ConfigFileDiagnosticsReporter {
        onUnRecoverableConfigFileDiagnostic: DiagnosticReporter;
    }
    interface ParseConfigFileHost extends ParseConfigHost, ConfigFileDiagnosticsReporter {
        getCurrentDirectory(): string;
    }
    interface ParsedTsconfig {
        raw: any;
        options?: CompilerOptions;
        watchOptions?: WatchOptions;
        typeAcquisition?: TypeAcquisition;
        extendedConfigPath?: string | string[];
    }
    interface ExtendedConfigCacheEntry {
        extendedResult: TsConfigSourceFile;
        extendedConfig: ParsedTsconfig | undefined;
    }
    function getEffectiveTypeRoots(options: CompilerOptions, host: GetEffectiveTypeRootsHost): string[] | undefined;
    function resolveTypeReferenceDirective(typeReferenceDirectiveName: string, containingFile: string | undefined, options: CompilerOptions, host: ModuleResolutionHost, redirectedReference?: ResolvedProjectReference, cache?: TypeReferenceDirectiveResolutionCache, resolutionMode?: ResolutionMode): ResolvedTypeReferenceDirectiveWithFailedLookupLocations;
    function getAutomaticTypeDirectiveNames(options: CompilerOptions, host: ModuleResolutionHost): string[];
    function createModuleResolutionCache(currentDirectory: string, getCanonicalFileName: (s: string) => string, options?: CompilerOptions, packageJsonInfoCache?: PackageJsonInfoCache): ModuleResolutionCache;
    function createTypeReferenceDirectiveResolutionCache(currentDirectory: string, getCanonicalFileName: (s: string) => string, options?: CompilerOptions, packageJsonInfoCache?: PackageJsonInfoCache): TypeReferenceDirectiveResolutionCache;
    function resolveModuleNameFromCache(moduleName: string, containingFile: string, cache: ModuleResolutionCache, mode?: ResolutionMode): ResolvedModuleWithFailedLookupLocations | undefined;
    function resolveModuleName(moduleName: string, containingFile: string, compilerOptions: CompilerOptions, host: ModuleResolutionHost, cache?: ModuleResolutionCache, redirectedReference?: ResolvedProjectReference, resolutionMode?: ResolutionMode): ResolvedModuleWithFailedLookupLocations;
    function bundlerModuleNameResolver(moduleName: string, containingFile: string, compilerOptions: CompilerOptions, host: ModuleResolutionHost, cache?: ModuleResolutionCache, redirectedReference?: ResolvedProjectReference): ResolvedModuleWithFailedLookupLocations;
    function nodeModuleNameResolver(moduleName: string, containingFile: string, compilerOptions: CompilerOptions, host: ModuleResolutionHost, cache?: ModuleResolutionCache, redirectedReference?: ResolvedProjectReference): ResolvedModuleWithFailedLookupLocations;
    function classicNameResolver(moduleName: string, containingFile: string, compilerOptions: CompilerOptions, host: ModuleResolutionHost, cache?: NonRelativeModuleNameResolutionCache, redirectedReference?: ResolvedProjectReference): ResolvedModuleWithFailedLookupLocations;
    interface TypeReferenceDirectiveResolutionCache extends PerDirectoryResolutionCache<ResolvedTypeReferenceDirectiveWithFailedLookupLocations>, NonRelativeNameResolutionCache<ResolvedTypeReferenceDirectiveWithFailedLookupLocations>, PackageJsonInfoCache {
    }
    interface ModeAwareCache<T> {
        get(key: string, mode: ResolutionMode): T | undefined;
        set(key: string, mode: ResolutionMode, value: T): this;
        delete(key: string, mode: ResolutionMode): this;
        has(key: string, mode: ResolutionMode): boolean;
        forEach(cb: (elem: T, key: string, mode: ResolutionMode) => void): void;
        size(): number;
    }
    interface PerDirectoryResolutionCache<T> {
        getFromDirectoryCache(name: string, mode: ResolutionMode, directoryName: string, redirectedReference: ResolvedProjectReference | undefined): T | undefined;
        getOrCreateCacheForDirectory(directoryName: string, redirectedReference?: ResolvedProjectReference): ModeAwareCache<T>;
        clear(): void;
        update(options: CompilerOptions): void;
    }
    interface NonRelativeNameResolutionCache<T> {
        getFromNonRelativeNameCache(nonRelativeName: string, mode: ResolutionMode, directoryName: string, redirectedReference: ResolvedProjectReference | undefined): T | undefined;
        getOrCreateCacheForNonRelativeName(nonRelativeName: string, mode: ResolutionMode, redirectedReference?: ResolvedProjectReference): PerNonRelativeNameCache<T>;
        clear(): void;
        update(options: CompilerOptions): void;
    }
    interface PerNonRelativeNameCache<T> {
        get(directory: string): T | undefined;
        set(directory: string, result: T): void;
    }
    interface ModuleResolutionCache extends PerDirectoryResolutionCache<ResolvedModuleWithFailedLookupLocations>, NonRelativeModuleNameResolutionCache, PackageJsonInfoCache {
        getPackageJsonInfoCache(): PackageJsonInfoCache;
    }
    interface NonRelativeModuleNameResolutionCache extends NonRelativeNameResolutionCache<ResolvedModuleWithFailedLookupLocations>, PackageJsonInfoCache {
        getOrCreateCacheForModuleName(nonRelativeModuleName: string, mode: ResolutionMode, redirectedReference?: ResolvedProjectReference): PerModuleNameCache;
    }
    interface PackageJsonInfoCache {
        clear(): void;
    }
    type PerModuleNameCache = PerNonRelativeNameCache<ResolvedModuleWithFailedLookupLocations>;
    function visitNode<TIn extends Node | undefined, TVisited extends Node | undefined, TOut extends Node>(node: TIn, visitor: Visitor<NonNullable<TIn>, TVisited>, test: (node: Node) => node is TOut, lift?: (node: readonly Node[]) => Node): TOut | (TIn & undefined) | (TVisited & undefined);
    function visitNode<TIn extends Node | undefined, TVisited extends Node | undefined>(node: TIn, visitor: Visitor<NonNullable<TIn>, TVisited>, test?: (node: Node) => boolean, lift?: (node: readonly Node[]) => Node): Node | (TIn & undefined) | (TVisited & undefined);
    function visitNodes<TIn extends Node, TInArray extends NodeArray<TIn> | undefined, TOut extends Node>(nodes: TInArray, visitor: Visitor<TIn, Node | undefined>, test: (node: Node) => node is TOut, start?: number, count?: number): NodeArray<TOut> | (TInArray & undefined);
    function visitNodes<TIn extends Node, TInArray extends NodeArray<TIn> | undefined>(nodes: TInArray, visitor: Visitor<TIn, Node | undefined>, test?: (node: Node) => boolean, start?: number, count?: number): NodeArray<Node> | (TInArray & undefined);
    function visitLexicalEnvironment(statements: NodeArray<Statement>, visitor: Visitor, context: TransformationContext, start?: number, ensureUseStrict?: boolean, nodesVisitor?: NodesVisitor): NodeArray<Statement>;
    function visitParameterList(nodes: NodeArray<ParameterDeclaration>, visitor: Visitor, context: TransformationContext, nodesVisitor?: NodesVisitor): NodeArray<ParameterDeclaration>;
    function visitParameterList(nodes: NodeArray<ParameterDeclaration> | undefined, visitor: Visitor, context: TransformationContext, nodesVisitor?: NodesVisitor): NodeArray<ParameterDeclaration> | undefined;
    function visitFunctionBody(node: FunctionBody, visitor: Visitor, context: TransformationContext): FunctionBody;
    function visitFunctionBody(node: FunctionBody | undefined, visitor: Visitor, context: TransformationContext): FunctionBody | undefined;
    function visitFunctionBody(node: ConciseBody, visitor: Visitor, context: TransformationContext): ConciseBody;
    function visitIterationBody(body: Statement, visitor: Visitor, context: TransformationContext): Statement;
    function visitCommaListElements(elements: NodeArray<Expression>, visitor: Visitor, discardVisitor?: Visitor): NodeArray<Expression>;
    function visitEachChild<T extends Node>(node: T, visitor: Visitor, context: TransformationContext | undefined): T;
    function visitEachChild<T extends Node>(node: T | undefined, visitor: Visitor, context: TransformationContext | undefined, nodesVisitor?: typeof visitNodes, tokenVisitor?: Visitor): T | undefined;
    function getTsBuildInfoEmitOutputFilePath(options: CompilerOptions): string | undefined;
    function getOutputFileNames(commandLine: ParsedCommandLine, inputFileName: string, ignoreCase: boolean): readonly string[];
    function createPrinter(printerOptions?: PrinterOptions, handlers?: PrintHandlers): Printer;
    enum ProgramUpdateLevel {
        Update = 0,
        RootNamesAndUpdate = 1,
        Full = 2,
    }
    function findConfigFile(searchPath: string, fileExists: (fileName: string) => boolean, configName?: string): string | undefined;
    function resolveTripleslashReference(moduleName: string, containingFile: string): string;
    function createCompilerHost(options: CompilerOptions, setParentNodes?: boolean): CompilerHost;
    function getPreEmitDiagnostics(program: Program, sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
    function formatDiagnostics(diagnostics: readonly Diagnostic[], host: FormatDiagnosticsHost): string;
    function formatDiagnostic(diagnostic: Diagnostic, host: FormatDiagnosticsHost): string;
    function formatDiagnosticsWithColorAndContext(diagnostics: readonly Diagnostic[], host: FormatDiagnosticsHost): string;
    function flattenDiagnosticMessageText(diag: string | DiagnosticMessageChain | undefined, newLine: string, indent?: number): string;
    function getModeForFileReference(ref: FileReference | string, containingFileMode: ResolutionMode): ResolutionMode;
    function getModeForResolutionAtIndex(file: SourceFile, index: number, compilerOptions: CompilerOptions): ResolutionMode;
    function getModeForUsageLocation(file: SourceFile, usage: StringLiteralLike, compilerOptions: CompilerOptions): ResolutionMode;
    function getConfigFileParsingDiagnostics(configFileParseResult: ParsedCommandLine): readonly Diagnostic[];
    function getImpliedNodeFormatForFile(fileName: string, packageJsonInfoCache: PackageJsonInfoCache | undefined, host: ModuleResolutionHost, options: CompilerOptions): ResolutionMode;
    function createProgram(createProgramOptions: CreateProgramOptions): Program;
    function createProgram(rootNames: readonly string[], options: CompilerOptions, host?: CompilerHost, oldProgram?: Program, configFileParsingDiagnostics?: readonly Diagnostic[]): Program;
    function resolveProjectReferencePath(ref: ProjectReference): ResolvedConfigFileName;
    interface FormatDiagnosticsHost {
        getCurrentDirectory(): string;
        getCanonicalFileName(fileName: string): string;
        getNewLine(): string;
    }
    interface EmitOutput {
        outputFiles: OutputFile[];
        emitSkipped: boolean;
        diagnostics: readonly Diagnostic[];
    }
    interface OutputFile {
        name: string;
        writeByteOrderMark: boolean;
        text: string;
    }
    function createSemanticDiagnosticsBuilderProgram(newProgram: Program, host: BuilderProgramHost, oldProgram?: SemanticDiagnosticsBuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[]): SemanticDiagnosticsBuilderProgram;
    function createSemanticDiagnosticsBuilderProgram(rootNames: readonly string[] | undefined, options: CompilerOptions | undefined, host?: CompilerHost, oldProgram?: SemanticDiagnosticsBuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[], projectReferences?: readonly ProjectReference[]): SemanticDiagnosticsBuilderProgram;
    function createEmitAndSemanticDiagnosticsBuilderProgram(newProgram: Program, host: BuilderProgramHost, oldProgram?: EmitAndSemanticDiagnosticsBuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[]): EmitAndSemanticDiagnosticsBuilderProgram;
    function createEmitAndSemanticDiagnosticsBuilderProgram(rootNames: readonly string[] | undefined, options: CompilerOptions | undefined, host?: CompilerHost, oldProgram?: EmitAndSemanticDiagnosticsBuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[], projectReferences?: readonly ProjectReference[]): EmitAndSemanticDiagnosticsBuilderProgram;
    function createAbstractBuilder(newProgram: Program, host: BuilderProgramHost, oldProgram?: BuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[]): BuilderProgram;
    function createAbstractBuilder(rootNames: readonly string[] | undefined, options: CompilerOptions | undefined, host?: CompilerHost, oldProgram?: BuilderProgram, configFileParsingDiagnostics?: readonly Diagnostic[], projectReferences?: readonly ProjectReference[]): BuilderProgram;
    type AffectedFileResult<T> = {
        result: T;
        affected: SourceFile | Program;
    } | undefined;
    interface BuilderProgramHost {
        createHash?: (data: string) => string;
        writeFile?: WriteFileCallback;
    }
    interface BuilderProgram {
        getProgram(): Program;
        getCompilerOptions(): CompilerOptions;
        getSourceFile(fileName: string): SourceFile | undefined;
        getSourceFiles(): readonly SourceFile[];
        getOptionsDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getGlobalDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getConfigFileParsingDiagnostics(): readonly Diagnostic[];
        getSyntacticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
        getDeclarationDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly DiagnosticWithLocation[];
        getAllDependencies(sourceFile: SourceFile): readonly string[];
        getSemanticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
        emit(targetSourceFile?: SourceFile, writeFile?: WriteFileCallback, cancellationToken?: CancellationToken, emitOnlyDtsFiles?: boolean, customTransformers?: CustomTransformers): EmitResult;
        getCurrentDirectory(): string;
    }
    interface SemanticDiagnosticsBuilderProgram extends BuilderProgram {
        getSemanticDiagnosticsOfNextAffectedFile(cancellationToken?: CancellationToken, ignoreSourceFile?: (sourceFile: SourceFile) => boolean): AffectedFileResult<readonly Diagnostic[]>;
    }
    interface EmitAndSemanticDiagnosticsBuilderProgram extends SemanticDiagnosticsBuilderProgram {
        emitNextAffectedFile(writeFile?: WriteFileCallback, cancellationToken?: CancellationToken, emitOnlyDtsFiles?: boolean, customTransformers?: CustomTransformers): AffectedFileResult<EmitResult>;
    }
    function readBuilderProgram(compilerOptions: CompilerOptions, host: ReadBuildProgramHost): EmitAndSemanticDiagnosticsBuilderProgram | undefined;
    function createIncrementalCompilerHost(options: CompilerOptions, system?: System): CompilerHost;
    function createIncrementalProgram<T extends BuilderProgram = EmitAndSemanticDiagnosticsBuilderProgram>({ rootNames, options, configFileParsingDiagnostics, projectReferences, host, createProgram }: IncrementalProgramOptions<T>): T;
    function createWatchCompilerHost<T extends BuilderProgram>(configFileName: string, optionsToExtend: CompilerOptions | undefined, system: System, createProgram?: CreateProgram<T>, reportDiagnostic?: DiagnosticReporter, reportWatchStatus?: WatchStatusReporter, watchOptionsToExtend?: WatchOptions, extraFileExtensions?: readonly FileExtensionInfo[]): WatchCompilerHostOfConfigFile<T>;
    function createWatchCompilerHost<T extends BuilderProgram>(rootFiles: string[], options: CompilerOptions, system: System, createProgram?: CreateProgram<T>, reportDiagnostic?: DiagnosticReporter, reportWatchStatus?: WatchStatusReporter, projectReferences?: readonly ProjectReference[], watchOptions?: WatchOptions): WatchCompilerHostOfFilesAndCompilerOptions<T>;
    function createWatchProgram<T extends BuilderProgram>(host: WatchCompilerHostOfFilesAndCompilerOptions<T>): WatchOfFilesAndCompilerOptions<T>;
    function createWatchProgram<T extends BuilderProgram>(host: WatchCompilerHostOfConfigFile<T>): WatchOfConfigFile<T>;
    interface ReadBuildProgramHost {
        useCaseSensitiveFileNames(): boolean;
        getCurrentDirectory(): string;
        readFile(fileName: string): string | undefined;
    }
    interface IncrementalProgramOptions<T extends BuilderProgram> {
        rootNames: readonly string[];
        options: CompilerOptions;
        configFileParsingDiagnostics?: readonly Diagnostic[];
        projectReferences?: readonly ProjectReference[];
        host?: CompilerHost;
        createProgram?: CreateProgram<T>;
    }
    type WatchStatusReporter = (diagnostic: Diagnostic, newLine: string, options: CompilerOptions, errorCount?: number) => void;
    type CreateProgram<T extends BuilderProgram> = (rootNames: readonly string[] | undefined, options: CompilerOptions | undefined, host?: CompilerHost, oldProgram?: T, configFileParsingDiagnostics?: readonly Diagnostic[], projectReferences?: readonly ProjectReference[] | undefined) => T;
    interface WatchHost {
        onWatchStatusChange?(diagnostic: Diagnostic, newLine: string, options: CompilerOptions, errorCount?: number): void;
        watchFile(path: string, callback: FileWatcherCallback, pollingInterval?: number, options?: WatchOptions): FileWatcher;
        watchDirectory(path: string, callback: DirectoryWatcherCallback, recursive?: boolean, options?: WatchOptions): FileWatcher;
        setTimeout?(callback: (...args: any[]) => void, ms: number, ...args: any[]): any;
        clearTimeout?(timeoutId: any): void;
        preferNonRecursiveWatch?: boolean;
    }
    interface ProgramHost<T extends BuilderProgram> {
        createProgram: CreateProgram<T>;
        useCaseSensitiveFileNames(): boolean;
        getNewLine(): string;
        getCurrentDirectory(): string;
        getDefaultLibFileName(options: CompilerOptions): string;
        getDefaultLibLocation?(): string;
        createHash?(data: string): string;
        fileExists(path: string): boolean;
        readFile(path: string, encoding?: string): string | undefined;
        directoryExists?(path: string): boolean;
        getDirectories?(path: string): string[];
        readDirectory?(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[];
        realpath?(path: string): string;
        trace?(s: string): void;
        getEnvironmentVariable?(name: string): string | undefined;
        resolveModuleNames?(moduleNames: string[], containingFile: string, reusedNames: string[] | undefined, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile?: SourceFile): (ResolvedModule | undefined)[];
        resolveTypeReferenceDirectives?(typeReferenceDirectiveNames: string[] | readonly FileReference[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingFileMode?: ResolutionMode): (ResolvedTypeReferenceDirective | undefined)[];
        resolveModuleNameLiterals?(moduleLiterals: readonly StringLiteralLike[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile, reusedNames: readonly StringLiteralLike[] | undefined): readonly ResolvedModuleWithFailedLookupLocations[];
        resolveTypeReferenceDirectiveReferences?<T extends FileReference | string>(typeDirectiveReferences: readonly T[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile | undefined, reusedNames: readonly T[] | undefined): readonly ResolvedTypeReferenceDirectiveWithFailedLookupLocations[];
        hasInvalidatedResolutions?(filePath: Path): boolean;
        getModuleResolutionCache?(): ModuleResolutionCache | undefined;
        jsDocParsingMode?: JSDocParsingMode;
    }
    interface WatchCompilerHost<T extends BuilderProgram> extends ProgramHost<T>, WatchHost {
        useSourceOfProjectReferenceRedirect?(): boolean;
        getParsedCommandLine?(fileName: string): ParsedCommandLine | undefined;
        afterProgramCreate?(program: T): void;
    }
    interface WatchCompilerHostOfFilesAndCompilerOptions<T extends BuilderProgram> extends WatchCompilerHost<T> {
        rootFiles: string[];
        options: CompilerOptions;
        watchOptions?: WatchOptions;
        projectReferences?: readonly ProjectReference[];
    }
    interface WatchCompilerHostOfConfigFile<T extends BuilderProgram> extends WatchCompilerHost<T>, ConfigFileDiagnosticsReporter {
        configFileName: string;
        optionsToExtend?: CompilerOptions;
        watchOptionsToExtend?: WatchOptions;
        extraFileExtensions?: readonly FileExtensionInfo[];
        readDirectory(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[];
    }
    interface Watch<T> {
        getProgram(): T;
        close(): void;
    }
    interface WatchOfConfigFile<T> extends Watch<T> {
    }
    interface WatchOfFilesAndCompilerOptions<T> extends Watch<T> {
        updateRootFileNames(fileNames: string[]): void;
    }
    function createBuilderStatusReporter(system: System, pretty?: boolean): DiagnosticReporter;
    function createSolutionBuilderHost<T extends BuilderProgram = EmitAndSemanticDiagnosticsBuilderProgram>(system?: System, createProgram?: CreateProgram<T>, reportDiagnostic?: DiagnosticReporter, reportSolutionBuilderStatus?: DiagnosticReporter, reportErrorSummary?: ReportEmitErrorSummary): SolutionBuilderHost<T>;
    function createSolutionBuilderWithWatchHost<T extends BuilderProgram = EmitAndSemanticDiagnosticsBuilderProgram>(system?: System, createProgram?: CreateProgram<T>, reportDiagnostic?: DiagnosticReporter, reportSolutionBuilderStatus?: DiagnosticReporter, reportWatchStatus?: WatchStatusReporter): SolutionBuilderWithWatchHost<T>;
    function createSolutionBuilder<T extends BuilderProgram>(host: SolutionBuilderHost<T>, rootNames: readonly string[], defaultOptions: BuildOptions): SolutionBuilder<T>;
    function createSolutionBuilderWithWatch<T extends BuilderProgram>(host: SolutionBuilderWithWatchHost<T>, rootNames: readonly string[], defaultOptions: BuildOptions, baseWatchOptions?: WatchOptions): SolutionBuilder<T>;
    interface BuildOptions {
        dry?: boolean;
        force?: boolean;
        verbose?: boolean;
        stopBuildOnErrors?: boolean;
        incremental?: boolean;
        assumeChangesOnlyAffectDirectDependencies?: boolean;
        declaration?: boolean;
        declarationMap?: boolean;
        emitDeclarationOnly?: boolean;
        sourceMap?: boolean;
        inlineSourceMap?: boolean;
        traceResolution?: boolean;
        [option: string]: CompilerOptionsValue | undefined;
    }
    type ReportEmitErrorSummary = (errorCount: number, filesInError: (ReportFileInError | undefined)[]) => void;
    interface ReportFileInError {
        fileName: string;
        line: number;
    }
    interface SolutionBuilderHostBase<T extends BuilderProgram> extends ProgramHost<T> {
        createDirectory?(path: string): void;
        writeFile?(path: string, data: string, writeByteOrderMark?: boolean): void;
        getCustomTransformers?: (project: string) => CustomTransformers | undefined;
        getModifiedTime(fileName: string): Date | undefined;
        setModifiedTime(fileName: string, date: Date): void;
        deleteFile(fileName: string): void;
        getParsedCommandLine?(fileName: string): ParsedCommandLine | undefined;
        reportDiagnostic: DiagnosticReporter;
        reportSolutionBuilderStatus: DiagnosticReporter;
        afterProgramEmitAndDiagnostics?(program: T): void;
    }
    interface SolutionBuilderHost<T extends BuilderProgram> extends SolutionBuilderHostBase<T> {
        reportErrorSummary?: ReportEmitErrorSummary;
    }
    interface SolutionBuilderWithWatchHost<T extends BuilderProgram> extends SolutionBuilderHostBase<T>, WatchHost {
    }
    interface SolutionBuilder<T extends BuilderProgram> {
        build(project?: string, cancellationToken?: CancellationToken, writeFile?: WriteFileCallback, getCustomTransformers?: (project: string) => CustomTransformers): ExitStatus;
        clean(project?: string): ExitStatus;
        buildReferences(project: string, cancellationToken?: CancellationToken, writeFile?: WriteFileCallback, getCustomTransformers?: (project: string) => CustomTransformers): ExitStatus;
        cleanReferences(project?: string): ExitStatus;
        getNextInvalidatedProject(cancellationToken?: CancellationToken): InvalidatedProject<T> | undefined;
    }
    enum InvalidatedProjectKind {
        Build = 0,
        UpdateOutputFileStamps = 1,
    }
    interface InvalidatedProjectBase {
        readonly kind: InvalidatedProjectKind;
        readonly project: ResolvedConfigFileName;
        done(cancellationToken?: CancellationToken, writeFile?: WriteFileCallback, customTransformers?: CustomTransformers): ExitStatus;
        getCompilerOptions(): CompilerOptions;
        getCurrentDirectory(): string;
    }
    interface UpdateOutputFileStampsProject extends InvalidatedProjectBase {
        readonly kind: InvalidatedProjectKind.UpdateOutputFileStamps;
        updateOutputFileStatmps(): void;
    }
    interface BuildInvalidedProject<T extends BuilderProgram> extends InvalidatedProjectBase {
        readonly kind: InvalidatedProjectKind.Build;
        getBuilderProgram(): T | undefined;
        getProgram(): Program | undefined;
        getSourceFile(fileName: string): SourceFile | undefined;
        getSourceFiles(): readonly SourceFile[];
        getOptionsDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getGlobalDiagnostics(cancellationToken?: CancellationToken): readonly Diagnostic[];
        getConfigFileParsingDiagnostics(): readonly Diagnostic[];
        getSyntacticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
        getAllDependencies(sourceFile: SourceFile): readonly string[];
        getSemanticDiagnostics(sourceFile?: SourceFile, cancellationToken?: CancellationToken): readonly Diagnostic[];
        getSemanticDiagnosticsOfNextAffectedFile(cancellationToken?: CancellationToken, ignoreSourceFile?: (sourceFile: SourceFile) => boolean): AffectedFileResult<readonly Diagnostic[]>;
        emit(targetSourceFile?: SourceFile, writeFile?: WriteFileCallback, cancellationToken?: CancellationToken, emitOnlyDtsFiles?: boolean, customTransformers?: CustomTransformers): EmitResult | undefined;
    }
    type InvalidatedProject<T extends BuilderProgram> = UpdateOutputFileStampsProject | BuildInvalidedProject<T>;
    function isBuildCommand(commandLineArgs: readonly string[]): boolean;
    function getDefaultFormatCodeSettings(newLineCharacter?: string): FormatCodeSettings;
    interface IScriptSnapshot {
        getText(start: number, end: number): string;
        getLength(): number;
        getChangeRange(oldSnapshot: IScriptSnapshot): TextChangeRange | undefined;
        dispose?(): void;
    }
    namespace ScriptSnapshot {
        function fromString(text: string): IScriptSnapshot;
    }
    interface PreProcessedFileInfo {
        referencedFiles: FileReference[];
        typeReferenceDirectives: FileReference[];
        libReferenceDirectives: FileReference[];
        importedFiles: FileReference[];
        ambientExternalModules?: string[];
        isLibFile: boolean;
    }
    interface HostCancellationToken {
        isCancellationRequested(): boolean;
    }
    interface InstallPackageOptions {
        fileName: Path;
        packageName: string;
    }
    interface PerformanceEvent {
        kind: "UpdateGraph" | "CreatePackageJsonAutoImportProvider";
        durationMs: number;
    }
    enum LanguageServiceMode {
        Semantic = 0,
        PartialSemantic = 1,
        Syntactic = 2,
    }
    interface IncompleteCompletionsCache {
        get(): CompletionInfo | undefined;
        set(response: CompletionInfo): void;
        clear(): void;
    }
    interface LanguageServiceHost extends GetEffectiveTypeRootsHost, MinimalResolutionCacheHost {
        getCompilationSettings(): CompilerOptions;
        getNewLine?(): string;
        getProjectVersion?(): string;
        getScriptFileNames(): string[];
        getScriptKind?(fileName: string): ScriptKind;
        getScriptVersion(fileName: string): string;
        getScriptSnapshot(fileName: string): IScriptSnapshot | undefined;
        getProjectReferences?(): readonly ProjectReference[] | undefined;
        getLocalizedDiagnosticMessages?(): any;
        getCancellationToken?(): HostCancellationToken;
        getCurrentDirectory(): string;
        getDefaultLibFileName(options: CompilerOptions): string;
        log?(s: string): void;
        trace?(s: string): void;
        error?(s: string): void;
        useCaseSensitiveFileNames?(): boolean;
        readDirectory?(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[];
        realpath?(path: string): string;
        readFile(path: string, encoding?: string): string | undefined;
        fileExists(path: string): boolean;
        getTypeRootsVersion?(): number;
        resolveModuleNames?(moduleNames: string[], containingFile: string, reusedNames: string[] | undefined, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile?: SourceFile): (ResolvedModule | undefined)[];
        getResolvedModuleWithFailedLookupLocationsFromCache?(modulename: string, containingFile: string, resolutionMode?: ResolutionMode): ResolvedModuleWithFailedLookupLocations | undefined;
        resolveTypeReferenceDirectives?(typeDirectiveNames: string[] | FileReference[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingFileMode?: ResolutionMode): (ResolvedTypeReferenceDirective | undefined)[];
        resolveModuleNameLiterals?(moduleLiterals: readonly StringLiteralLike[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile, reusedNames: readonly StringLiteralLike[] | undefined): readonly ResolvedModuleWithFailedLookupLocations[];
        resolveTypeReferenceDirectiveReferences?<T extends FileReference | string>(typeDirectiveReferences: readonly T[], containingFile: string, redirectedReference: ResolvedProjectReference | undefined, options: CompilerOptions, containingSourceFile: SourceFile | undefined, reusedNames: readonly T[] | undefined): readonly ResolvedTypeReferenceDirectiveWithFailedLookupLocations[];
        getDirectories?(directoryName: string): string[];
        getCustomTransformers?(): CustomTransformers | undefined;
        isKnownTypesPackageName?(name: string): boolean;
        installPackage?(options: InstallPackageOptions): Promise<ApplyCodeActionCommandResult>;
        writeFile?(fileName: string, content: string): void;
        getParsedCommandLine?(fileName: string): ParsedCommandLine | undefined;
        jsDocParsingMode?: JSDocParsingMode | undefined;
    }
    type WithMetadata<T> = T & {
        metadata?: unknown;
    };
    enum SemanticClassificationFormat {
        Original = "original",
        TwentyTwenty = "2020",
    }
    interface LanguageService {
        cleanupSemanticCache(): void;
        getSyntacticDiagnostics(fileName: string): DiagnosticWithLocation[];
        getSemanticDiagnostics(fileName: string): Diagnostic[];
        getSuggestionDiagnostics(fileName: string): DiagnosticWithLocation[];
        getCompilerOptionsDiagnostics(): Diagnostic[];
        getSyntacticClassifications(fileName: string, span: TextSpan): ClassifiedSpan[];
        getSyntacticClassifications(fileName: string, span: TextSpan, format: SemanticClassificationFormat): ClassifiedSpan[] | ClassifiedSpan2020[];
        getSemanticClassifications(fileName: string, span: TextSpan): ClassifiedSpan[];
        getSemanticClassifications(fileName: string, span: TextSpan, format: SemanticClassificationFormat): ClassifiedSpan[] | ClassifiedSpan2020[];
        getEncodedSyntacticClassifications(fileName: string, span: TextSpan): Classifications;
        getEncodedSemanticClassifications(fileName: string, span: TextSpan, format?: SemanticClassificationFormat): Classifications;
        getCompletionsAtPosition(fileName: string, position: number, options: GetCompletionsAtPositionOptions | undefined, formattingSettings?: FormatCodeSettings): WithMetadata<CompletionInfo> | undefined;
        getCompletionEntryDetails(fileName: string, position: number, entryName: string, formatOptions: FormatCodeOptions | FormatCodeSettings | undefined, source: string | undefined, preferences: UserPreferences | undefined, data: CompletionEntryData | undefined): CompletionEntryDetails | undefined;
        getCompletionEntrySymbol(fileName: string, position: number, name: string, source: string | undefined): Symbol | undefined;
        getQuickInfoAtPosition(fileName: string, position: number, maximumLength?: number): QuickInfo | undefined;
        getNameOrDottedNameSpan(fileName: string, startPos: number, endPos: number): TextSpan | undefined;
        getBreakpointStatementAtPosition(fileName: string, position: number): TextSpan | undefined;
        getSignatureHelpItems(fileName: string, position: number, options: SignatureHelpItemsOptions | undefined): SignatureHelpItems | undefined;
        getRenameInfo(fileName: string, position: number, preferences: UserPreferences): RenameInfo;
        getRenameInfo(fileName: string, position: number, options?: RenameInfoOptions): RenameInfo;
        findRenameLocations(fileName: string, position: number, findInStrings: boolean, findInComments: boolean, preferences: UserPreferences): readonly RenameLocation[] | undefined;
        findRenameLocations(fileName: string, position: number, findInStrings: boolean, findInComments: boolean, providePrefixAndSuffixTextForRename?: boolean): readonly RenameLocation[] | undefined;
        getSmartSelectionRange(fileName: string, position: number): SelectionRange;
        getDefinitionAtPosition(fileName: string, position: number): readonly DefinitionInfo[] | undefined;
        getDefinitionAndBoundSpan(fileName: string, position: number): DefinitionInfoAndBoundSpan | undefined;
        getTypeDefinitionAtPosition(fileName: string, position: number): readonly DefinitionInfo[] | undefined;
        getImplementationAtPosition(fileName: string, position: number): readonly ImplementationLocation[] | undefined;
        getReferencesAtPosition(fileName: string, position: number): ReferenceEntry[] | undefined;
        findReferences(fileName: string, position: number): ReferencedSymbol[] | undefined;
        getDocumentHighlights(fileName: string, position: number, filesToSearch: string[]): DocumentHighlights[] | undefined;
        getFileReferences(fileName: string): ReferenceEntry[];
        getNavigateToItems(searchValue: string, maxResultCount?: number, fileName?: string, excludeDtsFiles?: boolean, excludeLibFiles?: boolean): NavigateToItem[];
        getNavigationBarItems(fileName: string): NavigationBarItem[];
        getNavigationTree(fileName: string): NavigationTree;
        prepareCallHierarchy(fileName: string, position: number): CallHierarchyItem | CallHierarchyItem[] | undefined;
        provideCallHierarchyIncomingCalls(fileName: string, position: number): CallHierarchyIncomingCall[];
        provideCallHierarchyOutgoingCalls(fileName: string, position: number): CallHierarchyOutgoingCall[];
        provideInlayHints(fileName: string, span: TextSpan, preferences: UserPreferences | undefined): InlayHint[];
        getOutliningSpans(fileName: string): OutliningSpan[];
        getTodoComments(fileName: string, descriptors: TodoCommentDescriptor[]): TodoComment[];
        getBraceMatchingAtPosition(fileName: string, position: number): TextSpan[];
        getIndentationAtPosition(fileName: string, position: number, options: EditorOptions | EditorSettings): number;
        getFormattingEditsForRange(fileName: string, start: number, end: number, options: FormatCodeOptions | FormatCodeSettings): TextChange[];
        getFormattingEditsForDocument(fileName: string, options: FormatCodeOptions | FormatCodeSettings): TextChange[];
        getFormattingEditsAfterKeystroke(fileName: string, position: number, key: string, options: FormatCodeOptions | FormatCodeSettings): TextChange[];
        getDocCommentTemplateAtPosition(fileName: string, position: number, options?: DocCommentTemplateOptions, formatOptions?: FormatCodeSettings): TextInsertion | undefined;
        isValidBraceCompletionAtPosition(fileName: string, position: number, openingBrace: number): boolean;
        getJsxClosingTagAtPosition(fileName: string, position: number): JsxClosingTagInfo | undefined;
        getLinkedEditingRangeAtPosition(fileName: string, position: number): LinkedEditingInfo | undefined;
        getSpanOfEnclosingComment(fileName: string, position: number, onlyMultiLine: boolean): TextSpan | undefined;
        toLineColumnOffset?(fileName: string, position: number): LineAndCharacter;
        getCodeFixesAtPosition(fileName: string, start: number, end: number, errorCodes: readonly number[], formatOptions: FormatCodeSettings, preferences: UserPreferences): readonly CodeFixAction[];
        getCombinedCodeFix(scope: CombinedCodeFixScope, fixId: {}, formatOptions: FormatCodeSettings, preferences: UserPreferences): CombinedCodeActions;
        applyCodeActionCommand(action: CodeActionCommand, formatSettings?: FormatCodeSettings): Promise<ApplyCodeActionCommandResult>;
        applyCodeActionCommand(action: CodeActionCommand[], formatSettings?: FormatCodeSettings): Promise<ApplyCodeActionCommandResult[]>;
        applyCodeActionCommand(action: CodeActionCommand | CodeActionCommand[], formatSettings?: FormatCodeSettings): Promise<ApplyCodeActionCommandResult | ApplyCodeActionCommandResult[]>;
        applyCodeActionCommand(fileName: string, action: CodeActionCommand): Promise<ApplyCodeActionCommandResult>;
        applyCodeActionCommand(fileName: string, action: CodeActionCommand[]): Promise<ApplyCodeActionCommandResult[]>;
        applyCodeActionCommand(fileName: string, action: CodeActionCommand | CodeActionCommand[]): Promise<ApplyCodeActionCommandResult | ApplyCodeActionCommandResult[]>;
        getApplicableRefactors(fileName: string, positionOrRange: number | TextRange, preferences: UserPreferences | undefined, triggerReason?: RefactorTriggerReason, kind?: string, includeInteractiveActions?: boolean): ApplicableRefactorInfo[];
        getEditsForRefactor(fileName: string, formatOptions: FormatCodeSettings, positionOrRange: number | TextRange, refactorName: string, actionName: string, preferences: UserPreferences | undefined, interactiveRefactorArguments?: InteractiveRefactorArguments): RefactorEditInfo | undefined;
        getMoveToRefactoringFileSuggestions(fileName: string, positionOrRange: number | TextRange, preferences: UserPreferences | undefined, triggerReason?: RefactorTriggerReason, kind?: string): {
            newFileName: string;
            files: string[];
        };
        organizeImports(args: OrganizeImportsArgs, formatOptions: FormatCodeSettings, preferences: UserPreferences | undefined): readonly FileTextChanges[];
        getEditsForFileRename(oldFilePath: string, newFilePath: string, formatOptions: FormatCodeSettings, preferences: UserPreferences | undefined): readonly FileTextChanges[];
        getEmitOutput(fileName: string, emitOnlyDtsFiles?: boolean, forceDtsEmit?: boolean): EmitOutput;
        getProgram(): Program | undefined;
        toggleLineComment(fileName: string, textRange: TextRange): TextChange[];
        toggleMultilineComment(fileName: string, textRange: TextRange): TextChange[];
        commentSelection(fileName: string, textRange: TextRange): TextChange[];
        uncommentSelection(fileName: string, textRange: TextRange): TextChange[];
        getSupportedCodeFixes(fileName?: string): readonly string[];
        dispose(): void;
        preparePasteEditsForFile(fileName: string, copiedTextRanges: TextRange[]): boolean;
        getPasteEdits(args: PasteEditsArgs, formatOptions: FormatCodeSettings): PasteEdits;
    }
    interface JsxClosingTagInfo {
        readonly newText: string;
    }
    interface LinkedEditingInfo {
        readonly ranges: TextSpan[];
        wordPattern?: string;
    }
    interface CombinedCodeFixScope {
        type: "file";
        fileName: string;
    }
    enum OrganizeImportsMode {
        All = "All",
        SortAndCombine = "SortAndCombine",
        RemoveUnused = "RemoveUnused",
    }
    interface PasteEdits {
        edits: readonly FileTextChanges[];
        fixId?: {};
    }
    interface PasteEditsArgs {
        targetFile: string;
        pastedText: string[];
        pasteLocations: TextRange[];
        copiedFrom: {
            file: string;
            range: TextRange[];
        } | undefined;
        preferences: UserPreferences;
    }
    interface OrganizeImportsArgs extends CombinedCodeFixScope {
        skipDestructiveCodeActions?: boolean;
        mode?: OrganizeImportsMode;
    }
    type CompletionsTriggerCharacter = "." | '"' | "'" | "`" | "/" | "@" | "<" | "#" | " ";
    enum CompletionTriggerKind {
        Invoked = 1,
        TriggerCharacter = 2,
        TriggerForIncompleteCompletions = 3,
    }
    interface GetCompletionsAtPositionOptions extends UserPreferences {
        triggerCharacter?: CompletionsTriggerCharacter;
        triggerKind?: CompletionTriggerKind;
        includeSymbol?: boolean;
        includeExternalModuleExports?: boolean;
        includeInsertTextCompletions?: boolean;
    }
    type SignatureHelpTriggerCharacter = "," | "(" | "<";
    type SignatureHelpRetriggerCharacter = SignatureHelpTriggerCharacter | ")";
    interface SignatureHelpItemsOptions {
        triggerReason?: SignatureHelpTriggerReason;
    }
    type SignatureHelpTriggerReason = SignatureHelpInvokedReason | SignatureHelpCharacterTypedReason | SignatureHelpRetriggeredReason;
    interface SignatureHelpInvokedReason {
        kind: "invoked";
        triggerCharacter?: undefined;
    }
    interface SignatureHelpCharacterTypedReason {
        kind: "characterTyped";
        triggerCharacter: SignatureHelpTriggerCharacter;
    }
    interface SignatureHelpRetriggeredReason {
        kind: "retrigger";
        triggerCharacter?: SignatureHelpRetriggerCharacter;
    }
    interface ApplyCodeActionCommandResult {
        successMessage: string;
    }
    interface Classifications {
        spans: number[];
        endOfLineState: EndOfLineState;
    }
    interface ClassifiedSpan {
        textSpan: TextSpan;
        classificationType: ClassificationTypeNames;
    }
    interface ClassifiedSpan2020 {
        textSpan: TextSpan;
        classificationType: number;
    }
    interface NavigationBarItem {
        text: string;
        kind: ScriptElementKind;
        kindModifiers: string;
        spans: TextSpan[];
        childItems: NavigationBarItem[];
        indent: number;
        bolded: boolean;
        grayed: boolean;
    }
    interface NavigationTree {
        text: string;
        kind: ScriptElementKind;
        kindModifiers: string;
        spans: TextSpan[];
        nameSpan: TextSpan | undefined;
        childItems?: NavigationTree[];
    }
    interface CallHierarchyItem {
        name: string;
        kind: ScriptElementKind;
        kindModifiers?: string;
        file: string;
        span: TextSpan;
        selectionSpan: TextSpan;
        containerName?: string;
    }
    interface CallHierarchyIncomingCall {
        from: CallHierarchyItem;
        fromSpans: TextSpan[];
    }
    interface CallHierarchyOutgoingCall {
        to: CallHierarchyItem;
        fromSpans: TextSpan[];
    }
    enum InlayHintKind {
        Type = "Type",
        Parameter = "Parameter",
        Enum = "Enum",
    }
    interface InlayHint {
        text: string;
        position: number;
        kind: InlayHintKind;
        whitespaceBefore?: boolean;
        whitespaceAfter?: boolean;
        displayParts?: InlayHintDisplayPart[];
    }
    interface InlayHintDisplayPart {
        text: string;
        span?: TextSpan;
        file?: string;
    }
    interface TodoCommentDescriptor {
        text: string;
        priority: number;
    }
    interface TodoComment {
        descriptor: TodoCommentDescriptor;
        message: string;
        position: number;
    }
    interface TextChange {
        span: TextSpan;
        newText: string;
    }
    interface FileTextChanges {
        fileName: string;
        textChanges: readonly TextChange[];
        isNewFile?: boolean;
    }
    interface CodeAction {
        description: string;
        changes: FileTextChanges[];
        commands?: CodeActionCommand[];
    }
    interface CodeFixAction extends CodeAction {
        fixName: string;
        fixId?: {};
        fixAllDescription?: string;
    }
    interface CombinedCodeActions {
        changes: readonly FileTextChanges[];
        commands?: readonly CodeActionCommand[];
    }
    type CodeActionCommand = InstallPackageAction;
    interface InstallPackageAction {
    }
    interface ApplicableRefactorInfo {
        name: string;
        description: string;
        inlineable?: boolean;
        actions: RefactorActionInfo[];
    }
    interface RefactorActionInfo {
        name: string;
        description: string;
        notApplicableReason?: string;
        kind?: string;
        isInteractive?: boolean;
        range?: {
            start: {
                line: number;
                offset: number;
            };
            end: {
                line: number;
                offset: number;
            };
        };
    }
    interface RefactorEditInfo {
        edits: FileTextChanges[];
        renameFilename?: string;
        renameLocation?: number;
        commands?: CodeActionCommand[];
        notApplicableReason?: string;
    }
    type RefactorTriggerReason = "implicit" | "invoked";
    interface TextInsertion {
        newText: string;
        caretOffset: number;
    }
    interface DocumentSpan {
        textSpan: TextSpan;
        fileName: string;
        originalTextSpan?: TextSpan;
        originalFileName?: string;
        contextSpan?: TextSpan;
        originalContextSpan?: TextSpan;
    }
    interface RenameLocation extends DocumentSpan {
        readonly prefixText?: string;
        readonly suffixText?: string;
    }
    interface ReferenceEntry extends DocumentSpan {
        isWriteAccess: boolean;
        isInString?: true;
    }
    interface ImplementationLocation extends DocumentSpan {
        kind: ScriptElementKind;
        displayParts: SymbolDisplayPart[];
    }
    enum HighlightSpanKind {
        none = "none",
        definition = "definition",
        reference = "reference",
        writtenReference = "writtenReference",
    }
    interface HighlightSpan {
        fileName?: string;
        isInString?: true;
        textSpan: TextSpan;
        contextSpan?: TextSpan;
        kind: HighlightSpanKind;
    }
    interface NavigateToItem {
        name: string;
        kind: ScriptElementKind;
        kindModifiers: string;
        matchKind: "exact" | "prefix" | "substring" | "camelCase";
        isCaseSensitive: boolean;
        fileName: string;
        textSpan: TextSpan;
        containerName: string;
        containerKind: ScriptElementKind;
    }
    enum IndentStyle {
        None = 0,
        Block = 1,
        Smart = 2,
    }
    enum SemicolonPreference {
        Ignore = "ignore",
        Insert = "insert",
        Remove = "remove",
    }
    interface EditorOptions {
        BaseIndentSize?: number;
        IndentSize: number;
        TabSize: number;
        NewLineCharacter: string;
        ConvertTabsToSpaces: boolean;
        IndentStyle: IndentStyle;
    }
    interface EditorSettings {
        baseIndentSize?: number;
        indentSize?: number;
        tabSize?: number;
        newLineCharacter?: string;
        convertTabsToSpaces?: boolean;
        indentStyle?: IndentStyle;
        trimTrailingWhitespace?: boolean;
    }
    interface FormatCodeOptions extends EditorOptions {
        InsertSpaceAfterCommaDelimiter: boolean;
        InsertSpaceAfterSemicolonInForStatements: boolean;
        InsertSpaceBeforeAndAfterBinaryOperators: boolean;
        InsertSpaceAfterConstructor?: boolean;
        InsertSpaceAfterKeywordsInControlFlowStatements: boolean;
        InsertSpaceAfterFunctionKeywordForAnonymousFunctions: boolean;
        InsertSpaceAfterOpeningAndBeforeClosingNonemptyParenthesis: boolean;
        InsertSpaceAfterOpeningAndBeforeClosingNonemptyBrackets: boolean;
        InsertSpaceAfterOpeningAndBeforeClosingNonemptyBraces?: boolean;
        InsertSpaceAfterOpeningAndBeforeClosingTemplateStringBraces: boolean;
        InsertSpaceAfterOpeningAndBeforeClosingJsxExpressionBraces?: boolean;
        InsertSpaceAfterTypeAssertion?: boolean;
        InsertSpaceBeforeFunctionParenthesis?: boolean;
        PlaceOpenBraceOnNewLineForFunctions: boolean;
        PlaceOpenBraceOnNewLineForControlBlocks: boolean;
        insertSpaceBeforeTypeAnnotation?: boolean;
    }
    interface FormatCodeSettings extends EditorSettings {
        readonly insertSpaceAfterCommaDelimiter?: boolean;
        readonly insertSpaceAfterSemicolonInForStatements?: boolean;
        readonly insertSpaceBeforeAndAfterBinaryOperators?: boolean;
        readonly insertSpaceAfterConstructor?: boolean;
        readonly insertSpaceAfterKeywordsInControlFlowStatements?: boolean;
        readonly insertSpaceAfterFunctionKeywordForAnonymousFunctions?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingNonemptyParenthesis?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingNonemptyBrackets?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingNonemptyBraces?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingEmptyBraces?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingTemplateStringBraces?: boolean;
        readonly insertSpaceAfterOpeningAndBeforeClosingJsxExpressionBraces?: boolean;
        readonly insertSpaceAfterTypeAssertion?: boolean;
        readonly insertSpaceBeforeFunctionParenthesis?: boolean;
        readonly placeOpenBraceOnNewLineForFunctions?: boolean;
        readonly placeOpenBraceOnNewLineForControlBlocks?: boolean;
        readonly insertSpaceBeforeTypeAnnotation?: boolean;
        readonly indentMultiLineObjectLiteralBeginningOnBlankLine?: boolean;
        readonly semicolons?: SemicolonPreference;
        readonly indentSwitchCase?: boolean;
    }
    interface DefinitionInfo extends DocumentSpan {
        kind: ScriptElementKind;
        name: string;
        containerKind: ScriptElementKind;
        containerName: string;
        unverified?: boolean;
    }
    interface DefinitionInfoAndBoundSpan {
        definitions?: readonly DefinitionInfo[];
        textSpan: TextSpan;
    }
    interface ReferencedSymbolDefinitionInfo extends DefinitionInfo {
        displayParts: SymbolDisplayPart[];
    }
    interface ReferencedSymbol {
        definition: ReferencedSymbolDefinitionInfo;
        references: ReferencedSymbolEntry[];
    }
    interface ReferencedSymbolEntry extends ReferenceEntry {
        isDefinition?: boolean;
    }
    enum SymbolDisplayPartKind {
        aliasName = 0,
        className = 1,
        enumName = 2,
        fieldName = 3,
        interfaceName = 4,
        keyword = 5,
        lineBreak = 6,
        numericLiteral = 7,
        stringLiteral = 8,
        localName = 9,
        methodName = 10,
        moduleName = 11,
        operator = 12,
        parameterName = 13,
        propertyName = 14,
        punctuation = 15,
        space = 16,
        text = 17,
        typeParameterName = 18,
        enumMemberName = 19,
        functionName = 20,
        regularExpressionLiteral = 21,
        link = 22,
        linkName = 23,
        linkText = 24,
    }
    interface SymbolDisplayPart {
        text: string;
        kind: string;
    }
    interface JSDocLinkDisplayPart extends SymbolDisplayPart {
        target: DocumentSpan;
    }
    interface JSDocTagInfo {
        name: string;
        text?: SymbolDisplayPart[];
    }
    interface QuickInfo {
        kind: ScriptElementKind;
        kindModifiers: string;
        textSpan: TextSpan;
        displayParts?: SymbolDisplayPart[];
        documentation?: SymbolDisplayPart[];
        tags?: JSDocTagInfo[];
        canIncreaseVerbosityLevel?: boolean;
    }
    type RenameInfo = RenameInfoSuccess | RenameInfoFailure;
    interface RenameInfoSuccess {
        canRename: true;
        fileToRename?: string;
        displayName: string;
        fullDisplayName: string;
        kind: ScriptElementKind;
        kindModifiers: string;
        triggerSpan: TextSpan;
    }
    interface RenameInfoFailure {
        canRename: false;
        localizedErrorMessage: string;
    }
    interface RenameInfoOptions {
        readonly allowRenameOfImportPath?: boolean;
    }
    interface DocCommentTemplateOptions {
        readonly generateReturnInDocTemplate?: boolean;
    }
    interface InteractiveRefactorArguments {
        targetFile: string;
    }
    interface SignatureHelpParameter {
        name: string;
        documentation: SymbolDisplayPart[];
        displayParts: SymbolDisplayPart[];
        isOptional: boolean;
        isRest?: boolean;
    }
    interface SelectionRange {
        textSpan: TextSpan;
        parent?: SelectionRange;
    }
    interface SignatureHelpItem {
        isVariadic: boolean;
        prefixDisplayParts: SymbolDisplayPart[];
        suffixDisplayParts: SymbolDisplayPart[];
        separatorDisplayParts: SymbolDisplayPart[];
        parameters: SignatureHelpParameter[];
        documentation: SymbolDisplayPart[];
        tags: JSDocTagInfo[];
    }
    interface SignatureHelpItems {
        items: SignatureHelpItem[];
        applicableSpan: TextSpan;
        selectedItemIndex: number;
        argumentIndex: number;
        argumentCount: number;
    }
    enum CompletionInfoFlags {
        None = 0,
        MayIncludeAutoImports = 1,
        IsImportStatementCompletion = 2,
        IsContinuation = 4,
        ResolvedModuleSpecifiers = 8,
        ResolvedModuleSpecifiersBeyondLimit = 16,
        MayIncludeMethodSnippets = 32,
    }
    interface CompletionInfo {
        flags?: CompletionInfoFlags;
        isGlobalCompletion: boolean;
        isMemberCompletion: boolean;
        optionalReplacementSpan?: TextSpan;
        isNewIdentifierLocation: boolean;
        isIncomplete?: true;
        entries: CompletionEntry[];
        defaultCommitCharacters?: string[];
    }
    interface CompletionEntryDataAutoImport {
        exportName: string;
        exportMapKey?: ExportMapInfoKey;
        moduleSpecifier?: string;
        fileName?: string;
        ambientModuleName?: string;
        isPackageJsonImport?: true;
    }
    interface CompletionEntryDataUnresolved extends CompletionEntryDataAutoImport {
        exportMapKey: ExportMapInfoKey;
    }
    interface CompletionEntryDataResolved extends CompletionEntryDataAutoImport {
        moduleSpecifier: string;
    }
    type CompletionEntryData = CompletionEntryDataUnresolved | CompletionEntryDataResolved;
    interface CompletionEntry {
        name: string;
        kind: ScriptElementKind;
        kindModifiers?: string;
        sortText: string;
        insertText?: string;
        filterText?: string;
        isSnippet?: true;
        replacementSpan?: TextSpan;
        hasAction?: true;
        source?: string;
        sourceDisplay?: SymbolDisplayPart[];
        labelDetails?: CompletionEntryLabelDetails;
        isRecommended?: true;
        isFromUncheckedFile?: true;
        isPackageJsonImport?: true;
        isImportStatementCompletion?: true;
        symbol?: Symbol;
        data?: CompletionEntryData;
        commitCharacters?: string[];
    }
    interface CompletionEntryLabelDetails {
        detail?: string;
        description?: string;
    }
    interface CompletionEntryDetails {
        name: string;
        kind: ScriptElementKind;
        kindModifiers: string;
        displayParts: SymbolDisplayPart[];
        documentation?: SymbolDisplayPart[];
        tags?: JSDocTagInfo[];
        codeActions?: CodeAction[];
        source?: SymbolDisplayPart[];
        sourceDisplay?: SymbolDisplayPart[];
    }
    interface OutliningSpan {
        textSpan: TextSpan;
        hintSpan: TextSpan;
        bannerText: string;
        autoCollapse: boolean;
        kind: OutliningSpanKind;
    }
    enum OutliningSpanKind {
        Comment = "comment",
        Region = "region",
        Code = "code",
        Imports = "imports",
    }
    enum OutputFileType {
        JavaScript = 0,
        SourceMap = 1,
        Declaration = 2,
    }
    enum EndOfLineState {
        None = 0,
        InMultiLineCommentTrivia = 1,
        InSingleQuoteStringLiteral = 2,
        InDoubleQuoteStringLiteral = 3,
        InTemplateHeadOrNoSubstitutionTemplate = 4,
        InTemplateMiddleOrTail = 5,
        InTemplateSubstitutionPosition = 6,
    }
    enum TokenClass {
        Punctuation = 0,
        Keyword = 1,
        Operator = 2,
        Comment = 3,
        Whitespace = 4,
        Identifier = 5,
        NumberLiteral = 6,
        BigIntLiteral = 7,
        StringLiteral = 8,
        RegExpLiteral = 9,
    }
    interface ClassificationResult {
        finalLexState: EndOfLineState;
        entries: ClassificationInfo[];
    }
    interface ClassificationInfo {
        length: number;
        classification: TokenClass;
    }
    interface Classifier {
        getClassificationsForLine(text: string, lexState: EndOfLineState, syntacticClassifierAbsent: boolean): ClassificationResult;
        getEncodedLexicalClassifications(text: string, endOfLineState: EndOfLineState, syntacticClassifierAbsent: boolean): Classifications;
    }
    enum ScriptElementKind {
        unknown = "",
        warning = "warning",
        keyword = "keyword",
        scriptElement = "script",
        moduleElement = "module",
        classElement = "class",
        localClassElement = "local class",
        interfaceElement = "interface",
        typeElement = "type",
        enumElement = "enum",
        enumMemberElement = "enum member",
        variableElement = "var",
        localVariableElement = "local var",
        variableUsingElement = "using",
        variableAwaitUsingElement = "await using",
        functionElement = "function",
        localFunctionElement = "local function",
        memberFunctionElement = "method",
        memberGetAccessorElement = "getter",
        memberSetAccessorElement = "setter",
        memberVariableElement = "property",
        memberAccessorVariableElement = "accessor",
        constructorImplementationElement = "constructor",
        callSignatureElement = "call",
        indexSignatureElement = "index",
        constructSignatureElement = "construct",
        parameterElement = "parameter",
        typeParameterElement = "type parameter",
        primitiveType = "primitive type",
        label = "label",
        alias = "alias",
        constElement = "const",
        letElement = "let",
        directory = "directory",
        externalModuleName = "external module name",
        jsxAttribute = "JSX attribute",
        string = "string",
        link = "link",
        linkName = "link name",
        linkText = "link text",
    }
    enum ScriptElementKindModifier {
        none = "",
        publicMemberModifier = "public",
        privateMemberModifier = "private",
        protectedMemberModifier = "protected",
        exportedModifier = "export",
        ambientModifier = "declare",
        staticModifier = "static",
        abstractModifier = "abstract",
        optionalModifier = "optional",
        deprecatedModifier = "deprecated",
        dtsModifier = ".d.ts",
        tsModifier = ".ts",
        tsxModifier = ".tsx",
        jsModifier = ".js",
        jsxModifier = ".jsx",
        jsonModifier = ".json",
        dmtsModifier = ".d.mts",
        mtsModifier = ".mts",
        mjsModifier = ".mjs",
        dctsModifier = ".d.cts",
        ctsModifier = ".cts",
        cjsModifier = ".cjs",
    }
    enum ClassificationTypeNames {
        comment = "comment",
        identifier = "identifier",
        keyword = "keyword",
        numericLiteral = "number",
        bigintLiteral = "bigint",
        operator = "operator",
        stringLiteral = "string",
        whiteSpace = "whitespace",
        text = "text",
        punctuation = "punctuation",
        className = "class name",
        enumName = "enum name",
        interfaceName = "interface name",
        moduleName = "module name",
        typeParameterName = "type parameter name",
        typeAliasName = "type alias name",
        parameterName = "parameter name",
        docCommentTagName = "doc comment tag name",
        jsxOpenTagName = "jsx open tag name",
        jsxCloseTagName = "jsx close tag name",
        jsxSelfClosingTagName = "jsx self closing tag name",
        jsxAttribute = "jsx attribute",
        jsxText = "jsx text",
        jsxAttributeStringLiteralValue = "jsx attribute string literal value",
    }
    enum ClassificationType {
        comment = 1,
        identifier = 2,
        keyword = 3,
        numericLiteral = 4,
        operator = 5,
        stringLiteral = 6,
        regularExpressionLiteral = 7,
        whiteSpace = 8,
        text = 9,
        punctuation = 10,
        className = 11,
        enumName = 12,
        interfaceName = 13,
        moduleName = 14,
        typeParameterName = 15,
        typeAliasName = 16,
        parameterName = 17,
        docCommentTagName = 18,
        jsxOpenTagName = 19,
        jsxCloseTagName = 20,
        jsxSelfClosingTagName = 21,
        jsxAttribute = 22,
        jsxText = 23,
        jsxAttributeStringLiteralValue = 24,
        bigintLiteral = 25,
    }
    interface InlayHintsContext {
        file: SourceFile;
        program: Program;
        cancellationToken: CancellationToken;
        host: LanguageServiceHost;
        span: TextSpan;
        preferences: UserPreferences;
    }
    type ExportMapInfoKey = string & {
        __exportInfoKey: void;
    };
    function createClassifier(): Classifier;
    interface DocumentHighlights {
        fileName: string;
        highlightSpans: HighlightSpan[];
    }
    function createDocumentRegistry(useCaseSensitiveFileNames?: boolean, currentDirectory?: string, jsDocParsingMode?: JSDocParsingMode): DocumentRegistry;
    interface DocumentRegistry {
        acquireDocument(fileName: string, compilationSettingsOrHost: CompilerOptions | MinimalResolutionCacheHost, scriptSnapshot: IScriptSnapshot, version: string, scriptKind?: ScriptKind, sourceFileOptions?: CreateSourceFileOptions | ScriptTarget): SourceFile;
        acquireDocumentWithKey(fileName: string, path: Path, compilationSettingsOrHost: CompilerOptions | MinimalResolutionCacheHost, key: DocumentRegistryBucketKey, scriptSnapshot: IScriptSnapshot, version: string, scriptKind?: ScriptKind, sourceFileOptions?: CreateSourceFileOptions | ScriptTarget): SourceFile;
        updateDocument(fileName: string, compilationSettingsOrHost: CompilerOptions | MinimalResolutionCacheHost, scriptSnapshot: IScriptSnapshot, version: string, scriptKind?: ScriptKind, sourceFileOptions?: CreateSourceFileOptions | ScriptTarget): SourceFile;
        updateDocumentWithKey(fileName: string, path: Path, compilationSettingsOrHost: CompilerOptions | MinimalResolutionCacheHost, key: DocumentRegistryBucketKey, scriptSnapshot: IScriptSnapshot, version: string, scriptKind?: ScriptKind, sourceFileOptions?: CreateSourceFileOptions | ScriptTarget): SourceFile;
        getKeyForCompilationSettings(settings: CompilerOptions): DocumentRegistryBucketKey;
        releaseDocument(fileName: string, compilationSettings: CompilerOptions, scriptKind?: ScriptKind): void;
        releaseDocument(fileName: string, compilationSettings: CompilerOptions, scriptKind: ScriptKind, impliedNodeFormat: ResolutionMode): void;
        releaseDocumentWithKey(path: Path, key: DocumentRegistryBucketKey, scriptKind?: ScriptKind): void;
        releaseDocumentWithKey(path: Path, key: DocumentRegistryBucketKey, scriptKind: ScriptKind, impliedNodeFormat: ResolutionMode): void;
        reportStats(): string;
    }
    type DocumentRegistryBucketKey = string & {
        __bucketKey: any;
    };
    function preProcessFile(sourceText: string, readImportFiles?: boolean, detectJavaScriptImports?: boolean): PreProcessedFileInfo;
    function transpileModule(input: string, transpileOptions: TranspileOptions): TranspileOutput;
    function transpileDeclaration(input: string, transpileOptions: TranspileOptions): TranspileOutput;
    function transpile(input: string, compilerOptions?: CompilerOptions, fileName?: string, diagnostics?: Diagnostic[], moduleName?: string): string;
    interface TranspileOptions {
        compilerOptions?: CompilerOptions;
        fileName?: string;
        reportDiagnostics?: boolean;
        moduleName?: string;
        renamedDependencies?: MapLike<string>;
        transformers?: CustomTransformers;
        jsDocParsingMode?: JSDocParsingMode;
    }
    interface TranspileOutput {
        outputText: string;
        diagnostics?: Diagnostic[];
        sourceMapText?: string;
    }
    function toEditorSettings(options: EditorOptions | EditorSettings): EditorSettings;
    function displayPartsToString(displayParts: SymbolDisplayPart[] | undefined): string;
    function getDefaultCompilerOptions(): CompilerOptions;
    function getSupportedCodeFixes(): readonly string[];
    function createLanguageServiceSourceFile(fileName: string, scriptSnapshot: IScriptSnapshot, scriptTargetOrOptions: ScriptTarget | CreateSourceFileOptions, version: string, setNodeParents: boolean, scriptKind?: ScriptKind): SourceFile;
    function updateLanguageServiceSourceFile(sourceFile: SourceFile, scriptSnapshot: IScriptSnapshot, version: string, textChangeRange: TextChangeRange | undefined, aggressiveChecks?: boolean): SourceFile;
    function createLanguageService(host: LanguageServiceHost, documentRegistry?: DocumentRegistry, syntaxOnlyOrLanguageServiceMode?: boolean | LanguageServiceMode): LanguageService;
    function getDefaultLibFilePath(options: CompilerOptions): string;
    const servicesVersion = "0.8";
    function transform<T extends Node>(source: T | T[], transformers: TransformerFactory<T>[], compilerOptions?: CompilerOptions): TransformationResult<T>;
}
export = ts;
