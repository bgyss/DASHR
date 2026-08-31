
// These three things need to match - g_VertexInputDesc[], VertexBufferStruct, and VS_INPUT in VertexInput.hlsl
struct VS_INPUT
{
	float3 Pos : POSITION;
	float3 TexCoord : TEXCOORD0;
	float4 BoneWeights : TEXCOORD1;
	float3 Normal : TEXCOORD2;
	float3 Tangent : TEXCOORD3;
	float3 Bitangent : TEXCOORD4;
};

// These three things need to match - g_VertexInputWireframeDesc[], VertexBufferWireframeStruct, and VS_INPUT_Wireframe in VertexInput.hlsl
struct VS_INPUT_Wireframe
{
	float3 Pos : POSITION;
	float4 Colour : COLOR;
};


