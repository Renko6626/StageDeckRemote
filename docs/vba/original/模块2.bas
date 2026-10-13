Attribute VB_Name = "模块2"
Option Explicit

' 连线题VBA，左右选项相连
#If VBA7 Then
    Private Declare PtrSafe Function sndPlaySound Lib "winmm.dll" Alias "sndPlaySoundA" (ByVal lpszSoundName As String, ByVal uFlags As Long) As Long
    Private Declare PtrSafe Sub Sleep Lib "kernel32" (ByVal dwMilliseconds As Long)
#Else
    Private Declare Function sndPlaySound Lib "winmm.dll" Alias "sndPlaySoundA" (ByVal lpszSoundName As String, ByVal uFlags As Long) As Long
    Private Declare Sub Sleep Lib "kernel32" (ByVal dwMilliseconds As Long)
#End If

' 状态变量 (记录选中的左侧选项)
Dim selectedLeft As Shape

' 颜色配置 (&HBBGGRR 格式)
Const COLOR_DEFAULT = 0 ' 黑色
Const COLOR_SELECTED = &HF3FB4F     ' 黄色
Const COLOR_RIGHT = 5287936 ' 绿色
Const COLOR_WRONG = 255 ' 红色
Const TEXT_COLOR_DEFAULT = &H99E6FF

' 切换幻灯片自动重置
Sub OnSlideShowPageChange(ByVal Wn As SlideShowWindow)
    ResetAllShapes Wn.View.Slide
End Sub

' 辅助函数：设置样式
Private Sub SetShapeStyle(shp As Shape, fillColor As Long, transparency As Single, textColor As Long, isVivid As Boolean)
    On Error Resume Next
    shp.Fill.Visible = msoTrue
    shp.Fill.ForeColor.RGB = fillColor
    shp.Fill.transparency = transparency

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

' 重置函数：清除连线和颜色
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
        ' 查找以 Line_ 开头的形状或者包含 Left/Right 名称的选项
        If InStr(shp.Name, "Line_") > 0 Then
            shp.Delete
        ElseIf Left(shp.Name, 4) = "Left" Or Left(shp.Name, 5) = "Right" Then
            SetShapeStyle shp, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
        End If
    Next i
    Set selectedLeft = Nothing
End Sub

' 播放音频
Private Sub PlaySound(fileName As String)
    On Error Resume Next
    ' 请确保 PPT 目录下有 sound 文件夹且包含对应音频
    sndPlaySound ActivePresentation.path & "\sound\" & fileName, 1
    On Error GoTo 0
End Sub

' --- 核心逻辑 ---

' 点击左侧选项
Sub Click_Left(shpName As String)
    Dim sld As Slide: Set sld = ActivePresentation.SlideShowWindow.View.Slide

    ' 如果之前选过别的，先还原旧的
    If Not selectedLeft Is Nothing Then
        SetShapeStyle selectedLeft, COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
    End If

    ' 记录并高亮当前选中的左侧形状
    Set selectedLeft = sld.Shapes(shpName)
    SetShapeStyle selectedLeft, COLOR_SELECTED, 0.5, COLOR_SELECTED, True
End Sub

' 点击右侧选项
Sub Click_Right(shpID As Integer, shpName As String)
    If selectedLeft Is Nothing Then Exit Sub

    Dim sld As Slide: Set sld = ActivePresentation.SlideShowWindow.View.Slide
    Dim rightShp As Shape: Set rightShp = sld.Shapes(shpName)
    ' 从左侧形状名称中提取数字 ID (假设命名为 Left1, Left2...)
    Dim leftID As Integer: leftID = CInt(Replace(selectedLeft.Name, "Left", ""))
    Dim conn As Shape

    ' 添加连线
    Set conn = sld.Shapes.AddConnector(msoConnectorStraight, 0, 0, 100, 100)
    conn.Name = "Line_" & selectedLeft.Name & "_" & rightShp.Name

    ' 关键修改：左侧形状连接点为 4 (右中)，右侧形状连接点为 2 (左中)
    ' 注意：PowerPoint 默认矩形的连接点顺序通常是 1:上, 2:左, 3:下, 4:右
    conn.ConnectorFormat.BeginConnect selectedLeft, 4
    conn.ConnectorFormat.EndConnect rightShp, 2

    conn.Line.Weight = 5
    conn.ZOrder msoBringToFront

    If leftID = shpID Then
        ' 答对逻辑
        conn.Line.ForeColor.RGB = COLOR_RIGHT
        SetShapeStyle selectedLeft, COLOR_RIGHT, 0.5, COLOR_RIGHT, True
        SetShapeStyle rightShp, COLOR_RIGHT, 0.5, COLOR_RIGHT, True
        PlaySound "SE_Bingo.wav"
        Set selectedLeft = Nothing
    Else
        ' 答错逻辑
        conn.Line.ForeColor.RGB = COLOR_WRONG
        SetShapeStyle selectedLeft, COLOR_WRONG, 0.5, COLOR_WRONG, True
        SetShapeStyle rightShp, COLOR_WRONG, 0.5, COLOR_WRONG, True
        PlaySound "SE_Miss.wav"
        Set selectedLeft = Nothing
    End If
End Sub

' --- 宏绑定接口 ---

Sub Click_L1(): Click_Left "Left1": End Sub
Sub Click_R1(): Click_Right 1, "Right1": End Sub

Sub Click_L2(): Click_Left "Left2": End Sub
Sub Click_R2(): Click_Right 2, "Right2": End Sub

Sub Click_L3(): Click_Left "Left3": End Sub
Sub Click_R3(): Click_Right 3, "Right3": End Sub

Sub Click_L4(): Click_Left "Left4": End Sub
Sub Click_R4(): Click_Right 4, "Right4": End Sub

' --- 撤销功能 ---
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
                SetShapeStyle sld.Shapes(parts(1)), COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
                SetShapeStyle sld.Shapes(parts(2)), COLOR_DEFAULT, 0.8, TEXT_COLOR_DEFAULT, False
                shp.Delete
                Set selectedLeft = Nothing
                Exit Sub
            End If
        End If
    Next i
End Sub
