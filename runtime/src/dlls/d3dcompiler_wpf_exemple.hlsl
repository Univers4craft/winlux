//
//

//
// Samplers...
//

//
// Vertex Fragment Data...
//

struct Transform2D_VS_ConstData
{
    float4x4 mat4x4WorldToProjection;
};

//
// Vertex Shader Constant Data
//

struct VertexShaderConstantData
{
    Transform2D_VS_ConstData Transform2D_VS0_ConstantTable;
};

VertexShaderConstantData Data_VS;

//
// Pixel Fragment Data...
//


struct VertexShaderOutput
{
    float4 Position : POSITION;
    float4 Diffuse  : COLOR0;
    float4 UV_0 : TEXCOORD0;
};

//
// Fragment Vertex Shader functions...
//
void
Transform2D_VS0(
    float4 WorldPos2D,
    Transform2D_VS_ConstData Data,
    inout VertexShaderOutput Output
    )
{
    Output.Diffuse = float4(1.0, 1.0, 1.0, 1.0);
    Output.Position = mul(WorldPos2D, Data.mat4x4WorldToProjection);
}
void
MultiplyByInputDiffuse_VS1(
    float4 Diffuse,
    inout float4 BlendColor
    )
{
    BlendColor = Diffuse;
}


//
// Main Vertex Shader
//



VertexShaderOutput
VertexShaderImpl(
    float4 Position : POSITION,
    // Right now, only COLOR0 or NORMAL is used in a pass. The compiler
    // optimizes away what's not used.
    float4 Diffuse  : COLOR0,
    float3 Normal   : NORMAL,
    float2 UV_0     : TEXCOORD0,
    float2 UV_1     : TEXCOORD1
    )
{
    VertexShaderOutput Output = (VertexShaderOutput)0;

    // These will be optimized away when not in use
    float4x4 View, WorldView, WorldViewProj, WorldViewAdjTrans;
    float    SpecularPower;

    Transform2D_VS0(
        Position,
        Data_VS.Transform2D_VS0_ConstantTable,
        Output
        );

    MultiplyByInputDiffuse_VS1(
        Diffuse,
        Output.UV_0
        );

    Output.Diffuse.rgb = min(Output.Diffuse.rgb, 1.0);

    return Output;
};

//
// Fragment Pixel Shader fragments...
//
void
MultiplyByInputDiffuse_PS1(
    float4 BlendColor,
    inout float4 curPixelColor
    )
{
    curPixelColor *= BlendColor;
}


//
// Main Pixel Shader
//


float4
PixelShaderImpl(
    float4 Position : POSITION,
    float4 Diffuse  : COLOR0,
    float4 UV_0 : TEXCOORD0
    ) : COLOR
{
    float4 curColor = Diffuse;

    MultiplyByInputDiffuse_PS1(
        UV_0,
        curColor
        );

    return curColor;
};

//
// Technique
//

technique T0
{
    pass P0
    {
        VertexShader = compile vs_2_0 VertexShaderImpl();
        PixelShader  = compile ps_2_0 PixelShaderImpl();
    }
}

//
// End of Dynamic Shader Code
//
