Attribute VB_Name = "模块3"
Option Explicit

' 连线题VBA，上下选项相连，基本逻辑与模块2无区别
#If VBA7 Then
    Private Declare PtrSafe Function sndPlaySound Lib "winmm.dll" Alias "sndPlaySoundA" (ByVal lpszSoundName As String, ByVal uFlags As Long) As Long
    Private Declare PtrSafe Sub Sleep Lib "kernel32" (ByVal dwMilliseconds As Long)
#Else
    Private Declare Function sndPlaySound Lib "winmm.dll" Alias "sndPlaySoundA" (ByVal lpszSoundName As String, ByVal uFlags As Long) As Long
    Private Declare Sub Sleep Lib "kernel32" (ByVal dwMilliseconds As Long)
#End If

' 状态变量 (记录选中的上方选项)
Dim selectedTop As Shape

' 颜色配置 (&HBBGGRR 格式)
' Const COLOR_DEFAULT = 13421772 ' 灰色
Const COLOR_DEFAULT = 0 ' 黑色
Const COLOR_SELECTED = &HF3FB4F     ' 黄色
Const COLOR_RIGHT = 5287936 ' 绿色
Const COLOR_WRONG = 255 ' 红色
Const TEXT_COLOR_DEFAULT = &H99E6FF


Sub OnSlideShowPageChange(ByVal Wn As SlideShowWindow)
    ResetAllShapes Wn.View.Slide
End Sub

' 辅助函数
Private Sub SetShapeStyle(shp As Shape, fillColor As Long, transparency As Single, textColor As Long, isVivid As Boolean)
    On Error Resume Next
    ' 1. 设置填充和透明度
    shp.Fill.Visible = msoTrue
    shp.Fill.ForeColor.RGB = fillColor
    shp.Fill.transparency = transparency

    ' 2. 设置文字样式
    If shp.HasTextFrame Then
        If shp.TextFrame.HasText Then
            With shp.TextFrame.TextRange.Font
                .Color.RGB = textColor
                ' 如果要求鲜明 (isVivid 为 True)，则加粗并开启阴影
                If isVivid Then
                    .Bold = msoTrue
                    .Shadow = msoTrue
                    ' 如果你想让正确/错误时的文字比背景更亮，可以在这里微调
                Else
                    ' 重置状态：取消加粗，取消阴影
                    .Bold = msoFalse
                    .Shadow = msoFalse
                End If
            End With
        End If
    End If
    On Error GoTo 0
End Sub

' ==========================================
' 重置函数：安全清空连线和颜色
' ==========================================
Sub ResetAllShapes(Optional sld As Slide = Nothing)
    Dim shp As Shape
    Dim i As Integer
    If sld Is Nothing Then
        On Error Resume Next
        Set sld = ActivePresentation.SlideShowWindow.View.Slide
        On Error GoTo 0
    End If
    If sld Is Nothing Then Exit Sub

    For i = sld.Shapes.Count To 1 Step -1
        Set shp = sld.Shapes(i)
        If InStr(shp.Name, "Line_") > 0 Then
            shp.Delete
        ElseIf InStr(shp.Name, "Option") > 0 Or Left(shp.Name, 3) = "Top" Or Left(shp.Name, 6) = "Bottom" Then
            ' 重置时：不加粗，无阴影
            SetShapeStyle shp, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
        End If
    Next i
    Set selectedTop = Nothing
End Sub

'播放音频
Private Sub PlaySound(fileName As String)
    On Error Resume Next
    sndPlaySound ActivePresentation.path & "\sound\" & fileName, 1
    On Error GoTo 0
End Sub


' 连线题逻辑 (上下连线)

Sub Click_Top(shpName As String)
    Dim sld As Slide: Set sld = ActivePresentation.SlideShowWindow.View.Slide
    If Not selectedTop Is Nothing Then
        SetShapeStyle selectedTop, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
    End If
    Set selectedTop = sld.Shapes(shpName)
    ' 选中时也变鲜明，方便识别
    SetShapeStyle selectedTop, COLOR_SELECTED, 0.5, COLOR_SELECTED, True
End Sub

Sub Click_Bottom(shpID As Integer, shpName As String)
    If selectedTop Is Nothing Then Exit Sub
    Dim sld As Slide: Set sld = ActivePresentation.SlideShowWindow.View.Slide
    Dim bottomShp As Shape: Set bottomShp = sld.Shapes(shpName)
    Dim topID As Integer: topID = CInt(Replace(selectedTop.Name, "Top", ""))
    Dim conn As Shape

    Set conn = sld.Shapes.AddConnector(msoConnectorStraight, 0, 0, 100, 100)
    conn.Name = "Line_" & selectedTop.Name & "_" & bottomShp.Name
    conn.ConnectorFormat.BeginConnect selectedTop, 3
    conn.ConnectorFormat.EndConnect bottomShp, 1
    conn.Line.Weight = 5 ' 连线也可以稍微加粗一点
    conn.ZOrder msoBringToFront

    If topID = shpID Then
        conn.Line.ForeColor.RGB = COLOR_RIGHT
        ' 正确：鲜明化
        SetShapeStyle selectedTop, COLOR_RIGHT, 0.5, COLOR_RIGHT, True
        SetShapeStyle bottomShp, COLOR_RIGHT, 0.5, COLOR_RIGHT, True
        PlaySound "SE_Bingo.wav"
        Set selectedTop = Nothing
    Else
        conn.Line.ForeColor.RGB = COLOR_WRONG
        ' 错误：鲜明化
        SetShapeStyle selectedTop, COLOR_WRONG, 0.5, COLOR_WRONG, True
        SetShapeStyle bottomShp, COLOR_WRONG, 0.5, COLOR_WRONG, True
        PlaySound "SE_Miss.wav"
        Set selectedTop = Nothing
    End If
End Sub


' 绑定给具体形状的宏 (T代表Top，B代表Bottom)

Sub Click_T1(): Click_Top "Top1": End Sub
Sub Click_B1(): Click_Bottom 1, "Bottom1": End Sub

Sub Click_T2(): Click_Top "Top2": End Sub
Sub Click_B2(): Click_Bottom 2, "Bottom2": End Sub

Sub Click_T3(): Click_Top "Top3": End Sub
Sub Click_B3(): Click_Bottom 3, "Bottom3": End Sub

Sub Click_T4(): Click_Top "Top4": End Sub
Sub Click_B4(): Click_Bottom 4, "Bottom4": End Sub

' ==========================================
' 撤销按钮 (Back)
' ==========================================
Sub Click_Back()
    Dim sld As Slide: Dim shp As Shape: Dim i As Integer
    On Error Resume Next
    Set sld = ActivePresentation.SlideShowWindow.View.Slide
    If sld Is Nothing Then Exit Sub

    For i = sld.Shapes.Count To 1 Step -1
        Set shp = sld.Shapes(i)
        If InStr(shp.Name, "Line_") = 1 Then
            Dim parts() As String: parts = Split(shp.Name, "_")
            If UBound(parts) = 2 Then
                ' 撤销时还原：不加粗，无阴影，透明度0.8
                SetShapeStyle sld.Shapes(parts(1)), COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
                SetShapeStyle sld.Shapes(parts(2)), COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
                shp.Delete
                Set selectedTop = Nothing
                Exit Sub
            End If
        End If
    Next i
End Sub
