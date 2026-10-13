Attribute VB_Name = "模块1"
Option Explicit

' 多选题VBA逻辑
#If VBA7 Then
    Private Declare PtrSafe Function PlaySound Lib "winmm.dll" Alias "PlaySoundA" (ByVal lpszName As String, ByVal hModule As LongPtr, ByVal dwFlags As Long) As Long
#Else
    Private Declare Function PlaySound Lib "winmm.dll" Alias "PlaySoundA" (ByVal lpszName As String, ByVal hModule As Long, ByVal dwFlags As Long) As Long
#End If

' 常量定义
Const SND_ASYNC = &H1
Const SND_FILENAME = &H20000

' 颜色配置 (&HBBGGRR 格式)
Const COLOR_DEFAULT As Long = 0  ' 默认填充：黑
Const TEXT_COLOR_DEFAULT As Long = &H99E6FF
Const COLOR_SELECTED As Long = 65535    ' 选中：黄色
Const COLOR_CORRECT As Long = 5287936   ' 正确：绿色
Const COLOR_WRONG As Long = 255         ' 错误：红色

' ==========================================
' 辅助函数：统一设置形状样式 (填充、透明度、文字)
' ==========================================
Private Sub SetOptionStyle(shp As Shape, fillColor As Long, transparency As Single, textColor As Long, isVivid As Boolean)
    On Error Resume Next
    ' 1. 设置填充颜色和透明度
    shp.Fill.Visible = msoTrue
    shp.Fill.ForeColor.RGB = fillColor
    shp.Fill.transparency = transparency

    '设置边框
    shp.Line.Visible = msoTrue
    shp.Line.ForeColor.RGB = textColor

    ' 2. 设置文字样式
    If shp.HasTextFrame Then
        If shp.TextFrame.HasText Then
            With shp.TextFrame.TextRange.Font
                .Color.RGB = textColor
                If isVivid Then
                    .Bold = msoTrue
                    .Shadow = msoTrue
                Else
                    .Bold = msoFalse
                    .Shadow = msoFalse
                End If
            End With
        End If
    End If
    On Error GoTo 0
End Sub

' ==========================================
' 点击选项：选中/取消选中
' ==========================================
Sub ToggleOption(shp As Shape)
    If shp.Tags("Selected") = "Yes" Then
        ' 取消选中：恢复默认样式
        shp.Tags.Add "Selected", "No"
        SetOptionStyle shp, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
    Else
        ' 标记选中：变成选中样式（半透明黄色，加粗）
        shp.Tags.Add "Selected", "Yes"
        SetOptionStyle shp, COLOR_SELECTED, 0.6, COLOR_SELECTED, True
    End If
End Sub

' ==========================================
' 点击Check按钮：判断结果
' ==========================================
Sub CheckAnswer()
    Dim sld As Slide: Dim shp As Shape: Dim isCorrect As Boolean

    Set sld = SlideShowWindows(1).View.Slide
    isCorrect = True

    ' 1. 逻辑判断
    For Each shp In sld.Shapes
        If Left(shp.Name, 5) = "OptT_" Then
            If shp.Tags("Selected") <> "Yes" Then isCorrect = False
        ElseIf Left(shp.Name, 5) = "OptF_" Then
            If shp.Tags("Selected") = "Yes" Then isCorrect = False
        End If
    Next shp

    ' 2. 根据结果更新样式并播放声音
    If isCorrect Then
        For Each shp In sld.Shapes
            If (Left(shp.Name, 5) = "OptT_" Or Left(shp.Name, 5) = "OptF_") And shp.Tags("Selected") = "Yes" Then
                ' 正确：变绿，鲜明化
                SetOptionStyle shp, COLOR_CORRECT, 0.6, COLOR_CORRECT, True
            End If
        Next shp
        PlaySound ActivePresentation.path & "\sound\SE_Bingo.wav", 0, SND_ASYNC Or SND_FILENAME
    Else
        For Each shp In sld.Shapes
            If (Left(shp.Name, 5) = "OptT_" Or Left(shp.Name, 5) = "OptF_") And shp.Tags("Selected") = "Yes" Then
                ' 错误：变红，鲜明化
                SetOptionStyle shp, COLOR_WRONG, 0.6, COLOR_WRONG, True
            End If
        Next shp
        PlaySound ActivePresentation.path & "\sound\SE_Miss.wav", 0, SND_ASYNC Or SND_FILENAME
    End If
End Sub

' ==========================================
' 重置逻辑
' ==========================================
Sub BtnReset()
    Call ResetCurrentSlideOptions(SlideShowWindows(1).View.Slide)
End Sub

Sub ResetCurrentSlideOptions(sld As Slide)
    Dim shp As Shape
    For Each shp In sld.Shapes
        If Left(shp.Name, 5) = "OptT_" Or Left(shp.Name, 5) = "OptF_" Then
            shp.Tags.Add "Selected", "No"
            ' 重置时：颜色恢复默认，透明度0.8，取消加粗和阴影
            SetOptionStyle shp, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
        End If
    Next shp
End Sub

' 自动触发重置
Sub OnSlideShowPageChange(ByVal Wn As SlideShowWindow)
    Dim sld As Slide: Set sld = Wn.View.Slide
    Dim shp As Shape: Dim isQuizSlide As Boolean: isQuizSlide = False

    For Each shp In sld.Shapes
        If Left(shp.Name, 5) = "OptT_" Or Left(shp.Name, 5) = "OptF_" Then
            isQuizSlide = True
            Exit For
        End If
    Next shp

    If isQuizSlide Then Call ResetCurrentSlideOptions(sld)
End Sub
